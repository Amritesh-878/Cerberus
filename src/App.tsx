import { listen } from "@tauri-apps/api/event";
import {
  batch,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  For,
  Match,
  on,
  onCleanup,
  onMount,
  Show,
  Switch,
  untrack,
} from "solid-js";

import {
  type Account,
  accountState,
  isNewer,
  isRemoved,
  savedColor,
  loadProfiles,
  pickFolder,
  type Profiles,
  resetProfiles,
  setSidebarWidth,
  setSplitShare,
  statusText,
} from "./accounts";
import { AccountsScreen } from "./AccountsScreen";
import { Approvals, type TabName } from "./Approvals";
import { createAlertsSetting } from "./alerting";
import { continueOnAccount, continueTargets } from "./carry";
import {
  type Board,
  cleared,
  exited,
  markOf,
  nextWaiting,
  opened,
  reported,
  startedTabs,
} from "./board";
import { applyEnvironment, type EnvironmentChange } from "./environment";
import { EnvironmentDialog } from "./EnvironmentDialog";
import { createHandChats } from "./handChat";
import { HandNote } from "./HandNote";
import { Home } from "./Home";
import { AccountDot, MenuIcon, PanelIcon } from "./icons";
import { shortcutFor } from "./keys";
import { type Launch, NewTab } from "./NewTab";
import {
  createMessaging,
  decide,
  discard,
  NOTE_TEXT,
  typeIn,
} from "./messaging";
import { createLiveFeed, dismissWarning, warningText } from "./live";
import { LiveBanner } from "./LiveBanner";
import { createNotifier, sendWarning } from "./notify";
import { type Anchor, onScreen, type Side } from "./place";
import { followOpenRequests } from "./open";
import { Panel } from "./Panel";
import { CREATE_PR } from "./panelData";
import { createPanelState } from "./panelPrefs";
import { listPrograms } from "./pty";
import {
  fromSaved,
  loadTree,
  onWriteStatus,
  resumeChat,
  saveTree,
  toSaved,
} from "./saved";
import { Sidebar } from "./Sidebar";
import { createSavedWidth } from "./savedWidth";
import { createSplit, dropsToSplit } from "./split";
import { createStatuses } from "./statuses";
import { setupGaps, type Step, stepFor } from "./steps";
import { installed, shellChoice, type Tab, tabFor } from "./tab";
import { ShortcutsSheet } from "./ShortcutsSheet";
import type { TabDragEvents } from "./tabDrag";
import { SidebarEdge, SplitEdge } from "./SidebarEdge";
import { menuAccount, type MenuPlace, TabMenu } from "./TabMenu";
import { ServiceForm } from "./ServiceForm";
import { ServiceOutput } from "./ServiceOutput";
import {
  act as actOnService,
  addService,
  createServicesFeed,
  foldServices,
  openServiceUrl,
  type ServiceId,
  type ServiceRow,
  updateService,
} from "./services";
import { ServicesSection } from "./ServicesSection";
import { TerminalView } from "./Terminal";
import {
  addProject,
  addTab,
  closeTab,
  emptyTree,
  moveToAccount,
  neighbour,
  planners,
  type ProjectId,
  removeProject,
  renameProject,
  renameTab,
  stepTab,
  tabOrder,
  type TabId,
  type Tree,
} from "./tree";
import { Walkthrough } from "./Walkthrough";
import { SIDEBAR_DEFAULT, SPLIT_DEFAULT } from "./widths";

type Page = "walkthrough" | "accounts";

interface Opening {
  projectId: ProjectId;
  anchor: Anchor | undefined;
  side: Side;
}

export function App() {
  const [page, setPage] = createSignal<Page>();
  // The step the walkthrough opens at, set when the start page sends the user to what is missing.
  const [setupFrom, setSetupFrom] = createSignal<Step>("install");
  // A setup tab opened from a page shows over it, so the page keeps its place underneath.
  const [overPage, setOverPage] = createSignal(false);
  // Setup is left only by its own Finish or Skip, so no click, key or request elsewhere ends it by mistake.
  const inSetup = () => page() === "walkthrough";
  const showPage = (next: Page | undefined) => {
    if (untrack(inSetup) && next !== "walkthrough") return;
    setPage(next);
    setOverPage(false);
  };
  const endSetup = () => {
    setPage(undefined);
    setOverPage(false);
  };
  // Signals, not a resource: an errored resource keeps throwing after a fresh value is set.
  const [profiles, setProfiles] = createSignal<Profiles>();
  const [loadError, setLoadError] = createSignal<string>();
  const loaded = (fresh: Profiles) => {
    if (fresh.setup !== null) {
      setSetupFrom(fresh.setup);
      showPage("walkthrough");
    }
    setLoadError(undefined);
    setProfiles(fresh);
  };
  const changed = (fresh: Profiles) => {
    if (isNewer(fresh, untrack(profiles))) setProfiles(fresh);
  };
  const fail = (error: unknown) => {
    setLoadError(String(error));
  };
  loadProfiles().then(loaded, fail);
  const alertsSetting = createAlertsSetting(profiles, changed);
  const panel = createPanelState(profiles, changed);
  const [programs, { refetch: recheckPrograms }] = createResource(listPrograms);
  // Solid types a resource error as `any`.
  const programsError = (): unknown => programs.error;
  // An errored resource throws on `latest`, which would blank the page that should show the error.
  const programList = () =>
    programs.state === "errored" ? undefined : programs.latest;
  const statuses = createStatuses();

  const [tree, setTree] = createSignal<Tree>(emptyTree());
  const [active, setActive] = createSignal<TabId>();
  const [opening, setOpening] = createSignal<Opening>();
  // Holds the tab it opened for, so switching tabs while it is open cannot move another chat.
  // `chosen` opens the menu at its confirm step, as a warning's Continue on does.
  const [pillMenu, setPillMenu] = createSignal<{
    at: MenuPlace;
    tab: TabId;
    chosen?: Account;
  }>();
  const [sheet, setSheet] = createSignal(false);
  const sidebarWidth = createSavedWidth({
    saved: () => profiles()?.sidebarWidth,
    fallback: SIDEBAR_DEFAULT,
    send: setSidebarWidth,
    changed,
  });
  const splitShare = createSavedWidth({
    saved: () => profiles()?.splitShare,
    fallback: SPLIT_DEFAULT,
    send: setSplitShare,
    changed,
  });
  const [drawer, setDrawer] = createSignal(false);
  const [tabsClosed, setTabsClosed] = createSignal(0);
  const [board, setBoard] = createSignal<Board>(new Map());
  // Tabs whose chat ended; the core runs the shell in them until Resume chat.
  const [ended, setEnded] = createSignal<ReadonlySet<TabId>>(new Set());
  const markEnded = (id: TabId, on: boolean) => {
    setEnded((all) => {
      if (all.has(id) === on) return all;
      const next = new Set(all);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  // Nothing is saved until the saved tree is in, or an early save would replace it with an empty one.
  const [restored, setRestored] = createSignal(false);
  const [treeError, setTreeError] = createSignal<string>();
  const [treeNotice, setTreeNotice] = createSignal<string>();
  const savedTree = loadTree();
  createEffect(() => {
    const ready = profiles();
    const choices = programs.state === "errored" ? [] : programList();
    if (!ready || !choices || untrack(restored)) return;
    savedTree.then(
      (loaded) => {
        if (untrack(restored)) return;
        setTree(fromSaved(loaded.tree, ready, choices));
        setEnded(new Set(loaded.ended));
        setTreeNotice(loaded.notice ?? undefined);
        setRestored(true);
      },
      (error: unknown) => {
        setTreeError(String(error));
        setRestored(true);
      },
    );
  });
  // A write error comes from the core as it happens, a retry that later succeeds included.
  const [writeError, setWriteError] = createSignal<string>();
  const [callError, setCallError] = createSignal<string>();
  const saveError = () => callError() ?? writeError();
  onMount(() => {
    const listening = onWriteStatus((error) => {
      setWriteError(error ?? undefined);
    });
    onCleanup(() => {
      void listening.then((stop) => {
        stop();
      });
    });
  });
  // One call at a time, so an older tree can never reach the core after a newer one.
  let saving: Promise<void> = Promise.resolve();
  createEffect(
    on(
      tree,
      (current) => {
        if (!restored() || treeError() !== undefined) return;
        const next = toSaved(current);
        saving = saving
          .then(() => saveTree(next))
          .then(
            () => {
              setCallError(undefined);
            },
            (error: unknown) => {
              setCallError(String(error));
            },
          );
      },
      { defer: true },
    ),
  );

  createEffect(
    on(
      [
        () => profiles() === undefined,
        page,
        () => opening() !== undefined,
        tabsClosed,
      ],
      () => {
        const ids = untrack(profiles)?.accounts.map((a) => a.id) ?? [];
        statuses.refresh(ids);
      },
    ),
  );

  const tabIds = createMemo(() => tree().tabs.map((t) => t.id));
  const tabById = (id: TabId) => tree().tabs.find((t) => t.id === id);
  const shown = () =>
    page() === undefined || overPage() ? active() : undefined;
  const shownTab = () => {
    const id = shown();
    return id === undefined ? undefined : tabById(id);
  };
  const messaging = createMessaging();
  const split = createSplit(shown);
  // A service's output shown beside the tab on screen, in place of a split of two tabs.
  const services = createServicesFeed();
  const [viewing, setViewing] = createSignal<ServiceId>();
  const [serviceForm, setServiceForm] = createSignal<{
    editing?: ServiceRow;
  }>();
  const viewed = () => {
    const id = viewing();
    return services.view().services.find((s) => s.id === id);
  };
  const beside = () => viewed() !== undefined && shown() !== undefined;
  // A string, since the first service's id is 0 and a keyed Show treats 0 as nothing to show.
  const viewedKey = createMemo(() => {
    const service = viewed();
    return service === undefined ? undefined : String(service.id);
  });
  createEffect(
    on(split.showing, (pair) => {
      if (pair) setViewing(undefined);
    }),
  );
  // A page such as Accounts takes the whole stage, so the output would cover it.
  createEffect(
    on(page, (open) => {
      if (open !== undefined) setViewing(undefined);
    }),
  );
  const onServiceError = (error: unknown) => {
    const id = shown();
    if (id !== undefined) setAlert(id, String(error));
    else console.error("service:", error);
  };
  let stage: HTMLDivElement | undefined;
  // Set while a tab is dragged out of the sidebar; `over` once it would split on release.
  const [dropping, setDropping] = createSignal<{ over: boolean }>();
  const overStage = (x: number, y: number) => {
    const area = stage?.getBoundingClientRect();
    return area !== undefined && dropsToSplit(x, y, area);
  };
  // Read on every move and on the drop, so it follows the tab shown right now.
  const dropInto = (id: TabId) => {
    const into = shown();
    return inSetup() || into === id ? undefined : into;
  };
  const dragTab = (id: TabId): TabDragEvents => {
    return {
      move: (x, y) => {
        setDropping(
          dropInto(id) === undefined ? undefined : { over: overStage(x, y) },
        );
      },
      drop: (x, y) => {
        setDropping(undefined);
        const into = dropInto(id);
        if (into !== undefined && overStage(x, y)) split.open(into, id);
      },
      cancel: () => {
        setDropping(undefined);
      },
    };
  };
  const tabName = (id: TabId): TabName => {
    const tab = tabById(id);
    const account = tab?.launch.account;
    const all = profiles();
    const project = tree().projects.find((p) => p.id === tab?.projectId);
    return {
      accountId: account?.id,
      account:
        all?.accounts.find((a) => a.id === account?.id)?.name ??
        account?.name ??
        "Default account",
      color: account && all ? savedColor(all, account.id) : undefined,
      place: tab
        ? `${project?.name ?? "Setup"} / ${tab.title}`
        : `tab ${String(id)}`,
    };
  };
  // The tab beside the shown one in a split starts too, so its half is never blank.
  const started = createMemo<ReadonlySet<TabId>>(
    (before) =>
      startedTabs(
        startedTabs(before, shown(), tabIds()),
        split.beside(),
        tabIds(),
      ),
    new Set(),
  );
  const startedIds = createMemo(() =>
    tabIds().filter((id) => started().has(id)),
  );
  createEffect(
    on(shown, (id) => {
      if (id !== undefined) setBoard((b) => opened(b, id));
    }),
  );
  // Each start of a tab's program; 0 holds it stopped, and a new number starts it again.
  const [runs, setRuns] = createSignal<ReadonlyMap<TabId, number>>(new Map());
  const runOf = (id: TabId) => runs().get(id) ?? 1;
  const setRun = (id: TabId, run: number) => {
    setRuns((all) => new Map(all).set(id, run));
  };
  const [alerts, setAlerts] = createSignal<ReadonlyMap<TabId, string>>(
    new Map(),
  );
  const setAlert = (id: TabId, alert: string | undefined) => {
    setAlerts((all) => {
      const next = new Map(all);
      if (alert === undefined) next.delete(id);
      else next.set(id, alert);
      return next;
    });
  };
  // A tab left on a removed account keeps its place but runs nothing until its chat moves.
  const onRemoved = (id: TabId) => {
    const account = tabById(id)?.launch.account;
    const current = profiles();
    return (
      account != null && current !== undefined && isRemoved(current, account.id)
    );
  };
  const isRunning = (id: TabId) =>
    started().has(id) &&
    runOf(id) !== 0 &&
    !onRemoved(id) &&
    !ended().has(id) &&
    markOf(board(), id)?.kind !== "done";

  const select = (id: TabId) => {
    if (untrack(inSetup)) {
      // Only setup's own tabs, such as the installer, open while it runs; they show over it.
      if (tabById(id)?.projectId !== null) return;
      setActive(id);
      setOverPage(true);
      setDrawer(false);
      return;
    }
    setActive(id);
    showPage(undefined);
    setDrawer(false);
  };
  const notify = createNotifier({
    tree,
    board,
    shown,
    alertsOn: alertsSetting.on,
    open: (id) => {
      setOpening(undefined);
      select(id);
    },
  });
  const live = createLiveFeed((warning) => {
    if (!alertsSetting.on()) return;
    const name =
      profiles()?.accounts.find((a) => a.id === warning.account)?.name ??
      "An account";
    sendWarning(
      { title: "Usage limit", body: warningText(name, warning) },
      warning.tab,
    );
  });

  // The account each moving tab's chat is going to, shown on its row and pane until it restarts.
  const [moving, setMoving] = createSignal<ReadonlyMap<TabId, string>>(
    new Map(),
  );
  const setMovingTab = (id: TabId, to: string | undefined) => {
    setMoving((all) => {
      const next = new Map(all);
      if (to === undefined) next.delete(id);
      else next.set(id, to);
      return next;
    });
  };
  const continueOn = (id: TabId, account: Account) => {
    if (moving().has(id)) return;
    setMovingTab(id, account.name);
    const initialRun = runOf(id);
    batch(() => {
      setRun(id, 0);
      setBoard((b) => cleared(b, id));
      setAlert(id, undefined);
      select(id);
    });
    const restart = () => {
      setMovingTab(id, undefined);
      setRun(id, initialRun + 1);
    };
    continueOnAccount(id, account.id).then(
      () => {
        setTree(moveToAccount(tree(), id, account));
        markEnded(id, false);
        restart();
      },
      (error: unknown) => {
        setAlert(id, `Could not continue on ${account.name}: ${String(error)}`);
        restart();
      },
    );
  };

  // A shell has no chat to carry, so a shell left on a removed account just starts again on another.
  const restartOn = (id: TabId, account: Account) => {
    batch(() => {
      setTree(moveToAccount(tree(), id, account));
      setRun(id, runOf(id) + 1);
    });
  };

  const resume = (id: TabId) => {
    resumeChat(id).then(
      () => {
        batch(() => {
          markEnded(id, false);
          setBoard((b) => cleared(b, id));
          setAlert(id, undefined);
          setRun(id, runOf(id) + 1);
          select(id);
        });
      },
      (error: unknown) => {
        setAlert(id, `Could not resume the chat: ${String(error)}`);
      },
    );
  };

  // The tab's chat ended by itself, so it now runs the shell until Resume chat.
  const chatEnded = (id: TabId) => {
    notify(id, { state: "ended" });
    markEnded(id, true);
    setBoard((b) => cleared(b, id));
  };
  const hands = createHandChats({
    tree,
    setTree,
    claude: () => installed(programList() ?? [], "claude"),
    runOf,
    setRun,
    isEnded: (id) => ended().has(id),
    markEnded,
    clear: (id) => setBoard((b) => cleared(b, id)),
    ended: chatEnded,
    setAlert,
  });
  const byHandShown = () => {
    const id = shown();
    return id !== undefined && hands.byHand(id) ? id : undefined;
  };

  const openSetupTab = (launch: Tab) => {
    const [next, id] = addTab(
      tree(),
      { projectId: null, parentId: null },
      launch,
    );
    setTree(next);
    setActive(id);
    setOverPage(page() !== undefined);
    setDrawer(false);
  };

  const launch = (projectId: ProjectId, chosen: Launch) => {
    const project = tree().projects.find((p) => p.id === projectId);
    const account =
      profiles()?.accounts.find((a) => a.id === chosen.accountId) ?? null;
    const tab =
      project && tabFor(chosen.choice, { account, cwd: project.folder });
    if (!tab) return;
    const [next, id] = addTab(
      tree(),
      { projectId, parentId: chosen.parentId },
      tab,
    );
    setTree(next);
    setOpening(undefined);
    select(id);
  };

  const close = (id: TabId) => {
    const closing = tabById(id);
    if (!closing) return;
    const setup = closing.projectId === null;
    const after = setup ? undefined : neighbour(tree(), id);
    setTree(closeTab(tree(), id));
    split.forget(id);
    hands.forget(id);
    if (active() === id) {
      setActive(after);
      if (setup) setOverPage(false);
    }
    setTabsClosed((n) => n + 1);
    void recheckPrograms();
  };

  const forgetProject = (id: ProjectId) => {
    const gone = tree().tabs.filter((t) => t.projectId === id);
    const next = removeProject(tree(), id);
    gone.forEach((t) => {
      split.forget(t.id);
    });
    const current = active();
    setTree(next);
    if (current !== undefined && !next.tabs.some((t) => t.id === current)) {
      setActive(tabOrder(next)[0]);
    }
    if (opening()?.projectId === id) setOpening(undefined);
    setTabsClosed((n) => n + 1);
  };

  // The last shown tab's project, else the last project: where the start page opens a new tab.
  const homeProject = () => {
    const id = active();
    const projectId = id === undefined ? undefined : tabById(id)?.projectId;
    const projects = tree().projects;
    return projects.find((p) => p.id === projectId) ?? projects.at(-1);
  };

  // Beside the project's +, else beside the shown tab's row, else centred when the sidebar is a closed drawer.
  const newTabIn = (projectId: ProjectId) => {
    const view = { width: innerWidth, height: innerHeight };
    const anchor = [
      document.querySelector(`[data-new-tab="${String(projectId)}"]`),
      document.querySelector(".tab-row.active"),
    ]
      .map((el) => el?.getBoundingClientRect())
      .find((rect) => rect !== undefined && onScreen(rect, view));
    setDrawer(false);
    setOpening({ projectId, anchor, side: "beside" });
  };

  const newProject = () => {
    pickFolder(profiles()?.startFolder ?? undefined).then(
      (folder) => {
        if (folder === null) return;
        const [next, id] = addProject(tree(), folder);
        setTree(next);
        showPage(undefined);
        newTabIn(id);
      },
      (error: unknown) => {
        console.error("could not open the folder picker:", error);
      },
    );
  };

  followOpenRequests({
    tree,
    shown,
    // A request waits in the core until setup ends, so it cannot open a tab past it.
    restored: () => restored() && !inSetup(),
    // A picker left open would cover the tab the command asked for.
    select: (id) => {
      setOpening(undefined);
      select(id);
    },
    newTabIn,
  });

  onMount(() => {
    // Capture phase, so a shortcut never reaches the terminal as input.
    const onKey = (event: KeyboardEvent) => {
      const shortcut = shortcutFor(event);
      if (!shortcut || profiles() === undefined) return;
      // The Edit environment dialog is modal: a shortcut behind it could close its tab and drop the edits.
      if (editing() !== undefined) return;
      event.preventDefault();
      event.stopPropagation();
      if (shortcut === "shortcuts") {
        setSheet((open) => !open);
        return;
      }
      if (shortcut === "panel") {
        if (!inSetup()) panel.toggle();
        return;
      }
      if (inSetup() && shortcut !== "closeTab") return;
      if (shortcut === "nextTab" || shortcut === "previousTab") {
        const id = stepTab(tree(), shown(), shortcut === "nextTab" ? 1 : -1);
        if (id !== undefined) select(id);
      } else if (shortcut === "closeTab") {
        const id = shown();
        if (id !== undefined) close(id);
      } else if (shortcut === "nextWaiting") {
        const id = nextWaiting(board(), tabOrder(tree()), shown());
        if (id !== undefined) select(id);
      } else {
        const projectId = shownTab()?.projectId ?? tree().projects[0]?.id;
        if (projectId === undefined) newProject();
        else newTabIn(projectId);
      }
    };
    window.addEventListener("keydown", onKey, true);
    // The macOS Help menu item sends this, since its Cmd+/ never reaches the page.
    const fromMenu = listen("show-shortcuts", () => setSheet((open) => !open));
    onCleanup(() => {
      window.removeEventListener("keydown", onKey, true);
      void fromMenu.then((stop) => {
        stop();
      });
    });
  });

  // Typed into a new shell beside the tab, never run: the user reads it and presses Enter.
  const createPr = (folder: string) => {
    const from = shownTab();
    const current = profiles();
    const shell = current && shellChoice(current, programList() ?? []);
    if (from?.projectId == null || !current || !shell) return;
    const account =
      current.accounts.find((a) => a.id === from.launch.account?.id) ?? null;
    const tab = tabFor(shell, { account, cwd: folder, input: CREATE_PR });
    if (!tab) return;
    const place = {
      projectId: from.projectId,
      parentId: from.parentId ?? from.id,
    };
    const [next, id] = addTab(tree(), place, tab);
    setTree(renameTab(next, id, "Create PR"));
    select(id);
  };

  // Each open pane's way to type into its program, for the panel's prefills.
  const typers = new Map<TabId, (text: string) => void>();
  const chatShown = () => {
    const tab = shownTab();
    return tab?.launch.program === "claude" && isRunning(tab.id)
      ? tab.id
      : undefined;
  };
  // Typed into the chat's input for the user to send; a shell tab or an ended chat gets nothing.
  const typeIntoChat = (text: string) => {
    const id = chatShown();
    if (id !== undefined) typers.get(id)?.(text);
  };

  // The tab whose Edit environment dialog is open, and the tabs whose change is still being applied.
  const [editing, setEditing] = createSignal<TabId>();
  const [applying, setApplying] = createSignal<ReadonlySet<TabId>>(new Set());
  const saveEnvironment = (id: TabId, change: EnvironmentChange) => {
    setApplying((all) => new Set(all).add(id));
    return applyEnvironment(environment, id, change).finally(() => {
      setApplying((all) => {
        const next = new Set(all);
        next.delete(id);
        return next;
      });
    });
  };
  const environment = {
    tree,
    setTree,
    runOf,
    setRun,
    typeInto: (id: TabId, text: string) => {
      if (tabById(id)?.launch.program === "claude" && isRunning(id)) {
        typers.get(id)?.(text);
      }
    },
    setAlert,
  };

  const startFresh = () => {
    resetProfiles().then(loaded, fail);
  };

  return (
    <Switch>
      <Match when={loadError()}>
        <main class="walkthrough">
          <h1>Cerberus</h1>
          <p class="error">{loadError()}</p>
          <p>
            Start fresh to set up your accounts again. The old file is kept
            beside the new one as <code>profiles.json.bad</code>, or{" "}
            <code>.bad2</code>, <code>.bad3</code> and so on when that name is
            taken.
          </p>
          <button type="button" onClick={startFresh}>
            Start fresh
          </button>
        </main>
      </Match>
      <Match when={restored() && profiles()}>
        {(loaded) => (
          <div
            class="app"
            classList={{ drawer: drawer() }}
            style={{
              "--sidebar-width": `${String(sidebarWidth.value())}px`,
            }}
          >
            <SidebarEdge
              width={sidebarWidth.value()}
              onWidth={sidebarWidth.drag}
              onDone={sidebarWidth.save}
            />
            <Sidebar
              locked={inSetup()}
              tree={tree()}
              profiles={loaded()}
              statuses={statuses}
              active={shown()}
              mark={(id) => markOf(board(), id)}
              running={isRunning}
              movingTo={(id) => moving().get(id)}
              ended={(id) => ended().has(id)}
              removed={onRemoved}
              onContinue={continueOn}
              onResume={resume}
              onSelect={select}
              onClose={close}
              onRename={(id, title) => setTree(renameTab(tree(), id, title))}
              onNewTab={(projectId, anchor) => {
                setDrawer(false);
                setOpening({ projectId, anchor, side: "beside" });
              }}
              onRenameProject={(id, name) =>
                setTree(renameProject(tree(), id, name))
              }
              onRemoveProject={forgetProject}
              onNewProject={newProject}
              onDragTab={dragTab}
              onAccounts={() => {
                showPage("accounts");
                setDrawer(false);
              }}
              services={
                <ServicesSection
                  view={services.view()}
                  onFold={(folded) => {
                    foldServices(folded).then(services.set, onServiceError);
                  }}
                  onAdd={() => setServiceForm({})}
                  onAct={(id, action) => {
                    if (action === "remove" && viewing() === id)
                      setViewing(undefined);
                    actOnService(action, id).then(services.set, onServiceError);
                  }}
                  onOutput={(id) => {
                    split.close();
                    setViewing(id);
                    setDrawer(false);
                  }}
                  onEdit={(service) => setServiceForm({ editing: service })}
                  onOpenUrl={(id) => {
                    openServiceUrl(id).catch(onServiceError);
                  }}
                />
              }
            />
            <Show when={drawer()}>
              <div class="scrim" onClick={() => setDrawer(false)} />
            </Show>
            <section class="main">
              <header class="bar">
                <button
                  type="button"
                  class="icon-button menu-button"
                  aria-label="Show tabs"
                  onClick={() => setDrawer((open) => !open)}
                >
                  <MenuIcon />
                </button>
                <Show
                  when={shownTab()}
                  fallback={
                    <span class="crumb-title">
                      {page() === "accounts"
                        ? "Accounts"
                        : page() === "walkthrough"
                          ? "Setup"
                          : "Cerberus"}
                    </span>
                  }
                >
                  {(tab) => (
                    <>
                      <span class="crumb">
                        {tree().projects.find((p) => p.id === tab().projectId)
                          ?.name ?? "Setup"}
                        {" / "}
                        <Show when={tab().parentId}>
                          {(parentId) => <>{tabById(parentId())?.title} / </>}
                        </Show>
                      </span>
                      <span class="crumb-title">{tab().title}</span>
                    </>
                  )}
                </Show>
                <span class="spacer" />
                <Show when={saveError()}>
                  {(error) => (
                    <span class="save-error" role="status" title={error()}>
                      Tabs not saved
                    </span>
                  )}
                </Show>
                <Show when={sidebarWidth.error()}>
                  {(error) => (
                    <span class="save-error" role="status" title={error()}>
                      Sidebar width not saved
                    </span>
                  )}
                </Show>
                <Show when={splitShare.error()}>
                  {(error) => (
                    <span class="save-error" role="status" title={error()}>
                      Split not saved
                    </span>
                  )}
                </Show>
                <Show when={panel.error()}>
                  {(error) => (
                    <span class="save-error" role="status" title={error()}>
                      Panel not saved
                    </span>
                  )}
                </Show>
                <Show when={alertsSetting.error()}>
                  {(error) => (
                    <span class="save-error" role="status" title={error()}>
                      Alerts setting not saved
                    </span>
                  )}
                </Show>
                <Show when={messaging.note(shown())}>
                  {(note) => (
                    <span
                      class="messages-note"
                      role="status"
                      title={NOTE_TEXT[note()].title}
                    >
                      {NOTE_TEXT[note()].label}
                    </span>
                  )}
                </Show>
                <Show when={shownTab()?.launch.account}>
                  {(account) => (
                    <button
                      type="button"
                      class="account-pill"
                      title={statusText(statuses.get(account().id))}
                      aria-haspopup="menu"
                      aria-expanded={pillMenu() !== undefined}
                      onClick={(event) => {
                        const box = event.currentTarget.getBoundingClientRect();
                        const id = shown();
                        if (id !== undefined)
                          setPillMenu({
                            at: { x: box.left, y: box.bottom },
                            tab: id,
                          });
                      }}
                    >
                      <AccountDot
                        id={account().id}
                        color={savedColor(loaded(), account().id)}
                      />
                      {loaded().accounts.find((a) => a.id === account().id)
                        ?.name ?? account().name}
                    </button>
                  )}
                </Show>
                <Show when={!inSetup()}>
                  <button
                    type="button"
                    class="icon-button"
                    aria-label="Right panel"
                    aria-pressed={panel.open()}
                    title="Show or hide the right panel"
                    onClick={() => {
                      panel.toggle();
                    }}
                  >
                    <PanelIcon />
                  </button>
                </Show>
              </header>
              <Show when={shownTab()?.launch.signIn && shownTab()}>
                {(tab) => (
                  <div class="sign-in-bar">
                    <span>
                      Signing in {tab().launch.account?.name}. Close this tab
                      when you're done.
                    </span>
                    <button
                      type="button"
                      onClick={() => {
                        close(tab().id);
                      }}
                    >
                      Done
                    </button>
                  </div>
                )}
              </Show>
              <Show
                when={(() => {
                  const id = shown();
                  return id === undefined ? undefined : alerts().get(id);
                })()}
              >
                {(alert) => (
                  <div class="tab-alert" role="alert">
                    <span>{alert()}</span>
                    <span class="spacer" />
                    <button
                      type="button"
                      onClick={() => {
                        const id = shown();
                        if (id !== undefined) setAlert(id, undefined);
                      }}
                    >
                      Dismiss
                    </button>
                  </div>
                )}
              </Show>
              <LiveBanner
                warnings={live().warnings}
                profiles={loaded()}
                statuses={statuses}
                tabTitle={(id) => tabById(id)?.title}
                onContinue={(id, account, box) => {
                  setPillMenu({
                    at: { x: box.left, y: box.bottom },
                    tab: id,
                    chosen: account,
                  });
                }}
                onDismiss={(account) => {
                  dismissWarning(account).catch((error: unknown) => {
                    console.error("could not dismiss the warning:", error);
                  });
                }}
              />
              <Show when={byHandShown()} keyed>
                {(id) => (
                  <HandNote
                    working={markOf(board(), id)?.kind === "working"}
                    onReopen={() => {
                      hands.reopen(id);
                    }}
                  />
                )}
              </Show>
              <Show when={split.showing()}>
                {(pair) => (
                  <div class="split-bar">
                    <span class="split-title">
                      {tabName(pair()[0]).place} and {tabName(pair()[1]).place}
                    </span>
                    <span class="spacer" />
                    <button
                      type="button"
                      class="secondary"
                      onClick={() => {
                        split.close();
                      }}
                    >
                      Close split
                    </button>
                  </div>
                )}
              </Show>
              <div
                class="stage"
                ref={(el) => {
                  stage = el;
                }}
                style={{
                  "--split-share": `${String(splitShare.value() / 10)}%`,
                }}
              >
                <For each={startedIds()}>
                  {(id) => (
                    <Show when={tabById(id)}>
                      {(tab) => (
                        <div
                          class="tab-view"
                          classList={{
                            shown: shown() === id || split.has(id),
                            "split-left":
                              split.side(id) === "left" ||
                              (beside() && shown() === id),
                            "split-right": split.side(id) === "right",
                          }}
                          onPointerDown={() => {
                            if (split.has(id) && shown() !== id) select(id);
                          }}
                        >
                          <Show
                            when={onRemoved(id) ? 0 : runOf(id)}
                            keyed
                            fallback={
                              <Show
                                when={moving().get(id)}
                                fallback={
                                  <Show when={onRemoved(id)}>
                                    <Show
                                      when={tab().launch.program === "claude"}
                                      fallback={
                                        <div class="pane-note removed-note">
                                          <p>
                                            {tab().launch.account?.name} was
                                            removed, so this shell closed.
                                          </p>
                                          <div class="removed-actions">
                                            <For each={loaded().accounts}>
                                              {(account) => (
                                                <button
                                                  type="button"
                                                  class="secondary"
                                                  onClick={() => {
                                                    restartOn(id, account);
                                                  }}
                                                >
                                                  Restart on {account.name}
                                                </button>
                                              )}
                                            </For>
                                          </div>
                                        </div>
                                      }
                                    >
                                      <div class="pane-note removed-note">
                                        <p>
                                          {tab().launch.account?.name} was
                                          removed, so this tab's program closed.
                                          Its chat is saved.
                                        </p>
                                        <button
                                          type="button"
                                          class="secondary"
                                          onClick={(event) => {
                                            const box =
                                              event.currentTarget.getBoundingClientRect();
                                            setPillMenu({
                                              at: {
                                                x: box.left,
                                                y: box.bottom + 4,
                                              },
                                              tab: id,
                                            });
                                          }}
                                        >
                                          Continue on another account…
                                        </button>
                                      </div>
                                    </Show>
                                  </Show>
                                }
                              >
                                {(to) => (
                                  <p class="pane-note">Moving to {to()}…</p>
                                )}
                              </Show>
                            }
                          >
                            <TerminalView
                              tab={id}
                              spec={tab().launch.spec}
                              kind={
                                tab().launch.program === "claude" &&
                                !ended().has(id)
                                  ? "claude"
                                  : "shell"
                              }
                              imagePasteKey={
                                ended().has(id)
                                  ? null
                                  : tab().launch.imagePasteKey
                              }
                              input={tab().launch.input}
                              active={shown() === id || split.has(id)}
                              focused={shown() === id}
                              onState={(state) => {
                                notify(id, state);
                                setBoard((b) =>
                                  reported(b, id, state, shown()),
                                );
                              }}
                              onExit={() => setBoard((b) => exited(b, id))}
                              onEnded={() => {
                                chatEnded(id);
                              }}
                              onChat={(chat) => {
                                hands.heard(id, chat);
                              }}
                              onTyper={(type) => {
                                typers.set(id, type);
                                // A restarted pane may have set its own already.
                                return () => {
                                  if (typers.get(id) === type)
                                    typers.delete(id);
                                };
                              }}
                            />
                          </Show>
                        </div>
                      )}
                    </Show>
                  )}
                </For>
                <Show when={viewedKey()} keyed>
                  {(key) => (
                    <div
                      class="tab-view shown service-view"
                      classList={{ "split-right": beside() }}
                    >
                      <div class="service-bar">
                        <span class="split-title">{viewed()?.name} output</span>
                        <span class="spacer" />
                        <button
                          type="button"
                          class="secondary"
                          onClick={() => {
                            setViewing(undefined);
                          }}
                        >
                          Close
                        </button>
                      </div>
                      <ServiceOutput service={Number(key)} />
                    </div>
                  )}
                </Show>
                <Show when={split.showing() || beside()}>
                  <SplitEdge
                    width={splitShare.value()}
                    area={() => stage?.getBoundingClientRect()}
                    onWidth={splitShare.drag}
                    onDone={splitShare.save}
                  />
                </Show>
                <Show when={dropping()}>
                  {(over) => (
                    <div class="split-drop" classList={{ over: over().over }}>
                      <span>Show side by side</span>
                    </div>
                  )}
                </Show>
                <Show when={page() !== undefined || shown() === undefined}>
                  <div class="page" hidden={shown() !== undefined}>
                    <Switch
                      fallback={
                        <Home
                          project={homeProject()?.name}
                          hasProjects={tree().projects.length > 0}
                          gaps={setupGaps(loaded(), programList())}
                          treeError={treeError()}
                          treeNotice={treeNotice()}
                          onNewTab={(anchor) => {
                            const project = homeProject();
                            if (project)
                              setOpening({
                                projectId: project.id,
                                anchor,
                                side: "below",
                              });
                          }}
                          onNewProject={newProject}
                          onSetup={(gap) => {
                            setSetupFrom(stepFor(gap));
                            showPage("walkthrough");
                          }}
                        />
                      }
                    >
                      <Match when={page() === "walkthrough"}>
                        <Walkthrough
                          profiles={loaded()}
                          programs={programList() ?? []}
                          programsError={programsError()}
                          statuses={statuses}
                          start={setupFrom()}
                          onProfiles={changed}
                          onOpen={openSetupTab}
                          onRecheckPrograms={() => void recheckPrograms()}
                          onFinish={endSetup}
                        />
                      </Match>
                      <Match when={page() === "accounts"}>
                        <AccountsScreen
                          profiles={loaded()}
                          programs={programList() ?? []}
                          statuses={statuses}
                          alerts={alertsSetting.on()}
                          onAlerts={alertsSetting.save}
                          onProfiles={changed}
                          onOpen={openSetupTab}
                          onClose={() => {
                            showPage(undefined);
                          }}
                        />
                      </Match>
                    </Switch>
                  </div>
                </Show>
              </div>
            </section>
            <Show when={panel.open() && !inSetup()}>
              <Panel
                tab={shownTab()}
                profiles={loaded()}
                statuses={statuses}
                state={panel}
                canType={chatShown() !== undefined}
                onCreatePr={createPr}
                onType={typeIntoChat}
                links={messaging.snapshot()}
                live={live()}
                onEditEnvironment={() => {
                  const id = shown();
                  if (id !== undefined && !applying().has(id)) setEditing(id);
                }}
              />
            </Show>
            <Show
              when={(() => {
                const id = editing();
                return id === undefined ? undefined : tabById(id);
              })()}
            >
              {(tab) => (
                <EnvironmentDialog
                  tab={tab()}
                  chatRunning={
                    tab().launch.program === "claude" && isRunning(tab().id)
                  }
                  busy={markOf(board(), tab().id)?.kind === "working"}
                  onSave={(change) => saveEnvironment(tab().id, change)}
                  onClose={() => {
                    setEditing(undefined);
                  }}
                />
              )}
            </Show>
            <Show when={serviceForm()}>
              {(form) => (
                <ServiceForm
                  editing={form().editing}
                  shells={(programList() ?? []).filter(
                    (p) => p.isShell && p.program !== null,
                  )}
                  onSave={(draft) => {
                    const editing = form().editing;
                    return (
                      editing
                        ? updateService(editing.id, draft)
                        : addService(draft)
                    ).then((view) => {
                      services.set(view);
                    });
                  }}
                  onClose={() => {
                    setServiceForm(undefined);
                  }}
                />
              )}
            </Show>
            <Show when={sheet()}>
              <ShortcutsSheet onClose={() => setSheet(false)} />
            </Show>
            <Approvals
              pending={messaging.pending()}
              name={tabName}
              decide={decide}
              typeIn={typeIn}
              discard={discard}
              onApproved={(from, to) => {
                // Delivery can take seconds, and either tab may have closed meanwhile.
                if (!tabById(from) || !tabById(to)) return;
                split.open(from, to);
                // The target, since a typed-in message waits there for Enter.
                select(to);
              }}
            />
            <Show
              when={(() => {
                const open = pillMenu();
                return open === undefined ? undefined : tabById(open.tab);
              })()}
            >
              {(tab) => (
                <TabMenu
                  title={tab().title}
                  chosen={pillMenu()?.chosen}
                  account={menuAccount(
                    loaded().accounts.find(
                      (a) => a.id === tab().launch.account?.id,
                    ) ?? tab().launch.account,
                    loaded(),
                    (id) => accountState(loaded(), statuses.get(id), id),
                  )}
                  targets={continueTargets(
                    tab(),
                    loaded().accounts,
                    statuses.get,
                  )}
                  running={isRunning(tab().id)}
                  busy={moving().has(tab().id)}
                  ended={ended().has(tab().id) && !onRemoved(tab().id)}
                  at={pillMenu()?.at ?? { x: 0, y: 0 }}
                  onContinue={(account) => {
                    // Read before closing, since closing unmounts what holds the tab.
                    const id = tab().id;
                    setPillMenu(undefined);
                    continueOn(id, account);
                  }}
                  onResume={() => {
                    const id = tab().id;
                    setPillMenu(undefined);
                    resume(id);
                  }}
                  onAccounts={() => {
                    setPillMenu(undefined);
                    showPage("accounts");
                  }}
                  onDismiss={() => {
                    setPillMenu(undefined);
                  }}
                />
              )}
            </Show>
            <Show when={opening()}>
              {(open) => (
                <Show
                  when={tree().projects.find((p) => p.id === open().projectId)}
                >
                  {(project) => (
                    <NewTab
                      project={project()}
                      planners={planners(tree(), project().id)}
                      profiles={loaded()}
                      programs={programList()}
                      programsError={programsError()}
                      statuses={statuses}
                      anchor={open().anchor}
                      side={open().side}
                      onLaunch={(chosen) => {
                        launch(project().id, chosen);
                      }}
                      onDismiss={() => {
                        setOpening(undefined);
                      }}
                    />
                  )}
                </Show>
              )}
            </Show>
          </div>
        )}
      </Match>
    </Switch>
  );
}
