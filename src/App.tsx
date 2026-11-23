import { listen } from "@tauri-apps/api/event";
import {
  batch,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  For,
  lazy,
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
  type AccountId,
  accountLogins,
  accountState,
  crossesOrganization,
  isNewer,
  isRemoved,
  savedColor,
  loadProfiles,
  pickFolder,
  preselectedAccount,
  type Profiles,
  resetProfiles,
  sameFolder,
  setSidebarCollapsed,
  setSidebarWidth,
  setSplitShare,
  statusText,
} from "./accounts";
import { AccountsScreen } from "./AccountsScreen";
import { Approvals, type TabName } from "./Approvals";
import { createAlertsSetting } from "./alerting";
import { continueOnAccount, tabHasChat, continueTargets } from "./carry";
import {
  type Board,
  cleared,
  exited,
  type LiveState,
  markOf,
  nextWaiting,
  opened,
  reported,
  startedTabs,
} from "./board";
import { type FoundChat, projectFor, resumeFound } from "./chatSearch";
import {
  addDirCommand,
  applyEnvironment,
  type EnvironmentChange,
  tabMoveFolder,
} from "./environment";
import { FindChat } from "./FindChat";
import { EnvironmentDialog } from "./EnvironmentDialog";
import { createHandChats } from "./handChat";
import {
  handoffMessage,
  handoffOffer,
  handoffWritten,
  newestMarkdown,
  pickHandoff,
} from "./handoff";
import { HandNote } from "./HandNote";
import { RemovedNote } from "./RemovedNote";
import { repointed } from "./repoint";
import { Home } from "./Home";
import { AccountDot, MenuIcon, PanelIcon, ProgramIcon } from "./icons";
import { shortcutFor } from "./keys";
import { type Launch, NewTab } from "./NewTab";
import { NoticePopup } from "./NoticePopup";
import {
  createMessaging,
  decide,
  discard,
  noteText,
  typeIn,
} from "./messaging";
import {
  createLiveFeed,
  dismissWarning,
  type LiveWarning,
  warningText,
} from "./live";
import { LiveBanner } from "./LiveBanner";
import { createNotifier, sendWarning } from "./notify";
import { type Anchor, onScreen, type Side } from "./place";
import { followOpenRequests } from "./open";
import { Panel } from "./Panel";
import { CREATE_PR } from "./panelData";
import { createPanelState } from "./panelPrefs";
import { listPrograms, type ProgramChoice } from "./pty";
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
import { createSplit, dropSide, type Side as Half } from "./split";
import { createStatuses } from "./statuses";
import { setupGaps, type Step, stepFor } from "./steps";
import { installed, shellChoice, type Tab, tabFor, tabKind } from "./tab";
import { ShortcutsSheet } from "./ShortcutsSheet";
import type { TabDragEvents } from "./tabDrag";
import { SidebarEdge, SplitEdge } from "./SidebarEdge";
import { menuAccount, type MenuPlace, TabMenu } from "./TabMenu";
import { TaskDialog } from "./TaskDialog";
import { LimitPopup } from "./LimitPopup";
import { AFTER_LIMIT, type LimitHit, limitOffers, LimitQueue } from "./limits";
import { openTask, plannerFor, Prefills, withProjectTasks } from "./startTask";
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
import { ServicesRailIcon, ServicesSection } from "./ServicesSection";
import { type Sender, TerminalView } from "./Terminal";
import {
  addProject,
  addTab,
  closeTab,
  emptyTree,
  folderName,
  moveToAccount,
  neighbour,
  planners,
  type ProjectId,
  removeProject,
  renameProject,
  renameTab,
  setFolders,
  setHandoff,
  stepTab,
  tabOrder,
  type TabId,
  type Tree,
} from "./tree";
import { Walkthrough } from "./Walkthrough";

// CodeMirror loads with the first file opened, so startup stays as small as before.
const FilePane = lazy(() => import("./FilePane"));
import {
  panelOverlays,
  RAIL_BELOW,
  RAIL_WIDTH,
  sidebarMode,
} from "./sidebarLayout";
import { SidebarRail } from "./SidebarRail";
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
  // The sidebar's own row menu, a layer like the tab menu above.
  const [sidebarMenu, setSidebarMenu] = createSignal(false);
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
  const [windowWidth, setWindowWidth] = createSignal(innerWidth);
  onMount(() => {
    const onResize = () => setWindowWidth(innerWidth);
    addEventListener("resize", onResize);
    onCleanup(() => {
      removeEventListener("resize", onResize);
    });
  });
  // The click's choice, ahead of the saved profiles while its save is on the way.
  const [collapsedNow, setCollapsedNow] = createSignal<boolean>();
  const collapsed = () =>
    collapsedNow() ?? profiles()?.sidebarCollapsed ?? false;
  // Opened by hand in a window too narrow for the sidebar; never saved.
  const [narrowOpen, setNarrowOpen] = createSignal(false);
  createEffect(() => {
    if (windowWidth() >= RAIL_BELOW) setNarrowOpen(false);
  });
  const mode = () => sidebarMode(windowWidth(), collapsed(), narrowOpen());
  let collapseSaves = 0;
  const setCollapsed = (next: boolean) => {
    if (windowWidth() < RAIL_BELOW) {
      setNarrowOpen(!next);
      return;
    }
    setCollapsedNow(next);
    collapseSaves += 1;
    const mine = collapseSaves;
    setSidebarCollapsed(next)
      .then(changed, (error: unknown) => {
        console.error("sidebar state not saved:", error);
      })
      .finally(() => {
        // Only the latest click lets go, so an earlier save settling cannot flick the sidebar back.
        if (mine === collapseSaves) setCollapsedNow(undefined);
      });
  };
  const toggleSidebar = () => {
    if (mode() === "drawer") setDrawer((open) => !open);
    else setCollapsed(mode() === "open");
  };
  // Raised on each rail click, so a second click on the same project reveals it again.
  const [revealed, setRevealed] = createSignal<{
    project: ProjectId;
    n: number;
  }>();
  const openProject = (project: ProjectId) => {
    setCollapsed(false);
    setRevealed((now) => ({ project, n: (now?.n ?? 0) + 1 }));
  };
  const sidebarPx = () => {
    const now = mode();
    if (now === "drawer") return 0;
    if (now === "rail") return RAIL_WIDTH;
    return Math.min(sidebarWidth.value(), windowWidth() / 2);
  };
  const panelOver = () =>
    panel.open() &&
    panelOverlays(
      windowWidth(),
      sidebarPx(),
      Math.max(240, Math.min(panel.width(), (windowWidth() * 2) / 5)),
    );
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
  // A file the shown chat wrote, in the same place as a service's output; one of the two at a time.
  const [fileOpen, setFileOpen] = createSignal<{ tab: TabId; path: string }>();
  const fileShown = () => {
    const file = fileOpen();
    return file && file.tab === shown() ? file : undefined;
  };
  const beside = () =>
    (viewed() !== undefined || fileShown() !== undefined) &&
    shown() !== undefined;
  // A string, since the first service's id is 0 and a keyed Show treats 0 as nothing to show.
  const viewedKey = createMemo(() => {
    const service = viewed();
    return service === undefined ? undefined : String(service.id);
  });
  createEffect(
    on(split.showing, (pair) => {
      if (pair) {
        setViewing(undefined);
        setFileOpen(undefined);
      }
    }),
  );
  // A page such as Accounts takes the whole stage, so the output would cover it.
  createEffect(
    on(page, (open) => {
      if (open !== undefined) {
        setViewing(undefined);
        setFileOpen(undefined);
      }
    }),
  );
  createEffect(
    on(viewing, (id) => {
      if (id !== undefined) setFileOpen(undefined);
    }),
  );
  // The pane shows one chat's file, so moving to another tab closes it.
  createEffect(
    on(shown, (id) => {
      if (fileOpen()?.tab !== id) setFileOpen(undefined);
    }),
  );
  const openFile = (tab: TabId, path: string) => {
    split.close();
    setViewing(undefined);
    setFileOpen({ tab, path });
    // Over a narrow chat the panel would cover the pane it just opened.
    if (panelOver()) panel.toggle();
  };
  const onServiceError = (error: unknown) => {
    const id = shown();
    if (id !== undefined) setAlert(id, String(error));
    else console.error("service:", error);
  };
  let stage: HTMLDivElement | undefined;
  // Set while a tab is dragged out of the sidebar; `side` is the half it would land on if released now.
  const [dropping, setDropping] = createSignal<{ side: Half | undefined }>();
  const overStage = (x: number, y: number) => {
    const area = stage?.getBoundingClientRect();
    return area === undefined ? undefined : dropSide(x, y, area);
  };
  // Read on every move and on the drop, so it follows the tab shown right now.
  const dropInto = (id: TabId) => {
    const into = shown();
    return inSetup() || into === id ? undefined : into;
  };
  // The dragged tab's row, drawn under the pointer while it moves.
  const [ghost, setGhost] = createSignal<{ id: TabId; x: number; y: number }>();
  const dragTab = (id: TabId): TabDragEvents => {
    return {
      move: (x, y) => {
        setGhost({ id, x, y });
        setDropping(
          dropInto(id) === undefined ? undefined : { side: overStage(x, y) },
        );
      },
      drop: (x, y) => {
        batch(() => {
          setGhost(undefined);
          setDropping(undefined);
        });
        const side = overStage(x, y);
        if (dropInto(id) !== undefined && side) split.place(id, side);
      },
      cancel: () => {
        batch(() => {
          setGhost(undefined);
          setDropping(undefined);
        });
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
    // A new run is a new chat, which has drawn nothing yet.
    lastState.delete(id);
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
  // The tab's own chat ended, so its menu offers Resume; a removed account's tab moves instead.
  const endedHere = (id: TabId) => ended().has(id) && !onRemoved(id);
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
  // Low-limit warnings whose popup waits for its tab to be shown; the core warns once per account per window.
  const [warnWaiting, setWarnWaiting] = createSignal<readonly LiveWarning[]>(
    [],
  );
  const waitWarning = (warning: LiveWarning) => {
    setWarnWaiting((all) => [
      ...all.filter((w) => w.account !== warning.account),
      warning,
    ]);
  };
  const warnDone = (account: AccountId) => {
    setWarnWaiting((all) => all.filter((w) => w.account !== account));
  };
  const live = createLiveFeed((warning) => {
    waitWarning(warning);
    // The popup is over that tab already, unless the window is behind another.
    const seen = shown() === warning.tab && document.hasFocus();
    if (!alertsSetting.on() || seen) return;
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
  // Why a chat could not move, shown as a popup so it is never missed.
  const [moveFailed, setMoveFailed] = createSignal<{
    account: string;
    reason: string;
  }>();
  // `afterLimit`: the move came from the limit popup or banner, so the chat gets `continue` typed, never sent.
  // `quiet`: one of many moves for an account's removal, so it neither jumps to the tab nor shows its own failure.
  const continueOn = (
    id: TabId,
    account: Account,
    afterLimit = false,
    quiet = false,
  ): Promise<boolean> => {
    if (moving().has(id)) return Promise.resolve(false);
    setMovingTab(id, account.name);
    const initialRun = runOf(id);
    // The pane stays, paused, so the chat resumes in the same terminal with its scroll kept.
    batch(() => {
      setBoard((b) => cleared(b, id));
      setAlert(id, undefined);
      if (!quiet) select(id);
    });
    const restart = () => {
      batch(() => {
        setMovingTab(id, undefined);
        setRun(id, initialRun + 1);
      });
    };
    return continueOnAccount(id, account.id).then(
      () => {
        setTree(moveToAccount(tree(), id, account));
        markEnded(id, false);
        if (afterLimit) prefills.set(id, AFTER_LIMIT);
        restart();
        return true;
      },
      (error: unknown) => {
        if (!quiet)
          setMoveFailed({ account: account.name, reason: String(error) });
        restart();
        return false;
      },
    );
  };

  // An account that came back on a removed one's folder takes its tabs, with no note: their chats are already there.
  createEffect(() => {
    const current = profiles();
    if (!current) return;
    const now = tree();
    const next = repointed(now, current);
    if (next !== now) setTree(next);
  });
  // A tab starting may follow a `/login` in its folder, so its account's sign-in and login are read again then.
  const [loginsChecked, setLoginsChecked] = createSignal(0);
  let runsSeen = new Map<TabId, number>();
  createEffect(() => {
    const now = new Map(startedIds().map((id) => [id, runOf(id)]));
    const started = new Set<AccountId>();
    for (const [id, run] of now) {
      if (run === 0 || runsSeen.get(id) === run) continue;
      const account = untrack(() => tabById(id)?.launch.account?.id);
      if (account != null) started.add(account);
    }
    runsSeen = now;
    if (started.size === 0) return;
    untrack(() => {
      statuses.refresh([...started]);
      setLoginsChecked((n) => n + 1);
    });
  });
  // Who shares a login or an organization, from each folder's own file as it is now, not as when it was added.
  const [logins] = createResource(() => {
    const current = profiles();
    return current && { current, checked: loginsChecked() };
  }, accountLogins);
  const defaultTarget = () => {
    const current = profiles();
    if (!current) return undefined;
    const id = preselectedAccount(current);
    return current.accounts.find((a) => a.id === id);
  };
  // A removed account's tab: its chat moves, or a shell starts again on the account picked.
  const moveRemoved = (id: TabId, to: Account) => {
    if (tabById(id)?.launch.program === "claude") void continueOn(id, to);
    else restartOn(id, to);
  };

  // A shell has no chat to carry, so a shell left on a removed account just starts again on another.
  const restartOn = (id: TabId, account: Account) => {
    batch(() => {
      setTree(moveToAccount(tree(), id, account));
      setRun(id, runOf(id) + 1);
    });
  };

  // Every tab on `from` goes to `to` before `from` is removed: one folder re-points, a chat moves, a tab with no chat starts there.
  const moveTabsOff = async (from: AccountId, to: Account) => {
    const source = profiles()?.accounts.find((a) => a.id === from);
    const ids = tree()
      .tabs.filter((t) => t.launch.account?.id === from)
      .map((t) => t.id);
    const stayed: string[] = [];
    for (const id of ids) {
      const tab = tabById(id);
      // A tab already moving finishes its own move.
      if (!tab || moving().has(id)) continue;
      if (source && sameFolder(source.configDir, to.configDir)) {
        setTree(moveToAccount(tree(), id, to));
      } else if (tab.launch.program !== "claude" || !(await tabHasChat(id))) {
        restartOn(id, to);
      } else if (!(await continueOn(id, to, false, true))) {
        stayed.push(tab.title);
      }
    }
    // A chat that couldn't move stays saved on its tab, which offers the move again once its account is gone.
    if (stayed.length > 0) {
      setMoveFailed({
        account: to.name,
        reason: `${stayed.join(", ")} couldn't move`,
      });
    }
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
  // Only a `claude` that bypassed the wrapper lacks the tab's links; a removed account's tab runs nothing at all.
  const handBarShown = () => {
    const id = shown();
    return id !== undefined &&
      hands.byHand(id) &&
      hands.unwrapped(id) &&
      !onRemoved(id)
      ? id
      : undefined;
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

  const [finding, setFinding] = createSignal(false);
  // A found chat: its tab if it has one, else a new tab that resumes it in its folder on its account.
  const openFound = async (chat: FoundChat, folder?: string) => {
    if (chat.openIn !== null && tabById(chat.openIn)) {
      select(chat.openIn);
      return;
    }
    const claude = programList()?.find((p) => p.id === "claude");
    const account =
      chat.account === null
        ? null
        : (profiles()?.accounts.find((a) => a.id === chat.account) ?? null);
    if (chat.account !== null && account === null) {
      throw new Error("its account is no longer set up");
    }
    // It starts out in the chat's own folder, where the core finds its file; a chosen folder comes after the move.
    const own = chat.folder ?? folder;
    const tab = claude && tabFor(claude, { account, cwd: own ?? null });
    if (!tab || own === undefined)
      throw new Error("Claude Code is not installed");
    const [withProject, projectId] = projectFor(tree(), folder ?? own);
    const madeProject = !tree().projects.some((p) => p.id === projectId);
    const [added, id] = addTab(withProject, { projectId, parentId: null }, tab);
    setRun(id, 0);
    // The default name, since a tab's name is saved to disk and the chat's title is transcript text.
    setTree(added);
    try {
      // The core resumes only a tab it has saved, so the save this change queued goes first.
      await saving;
      await resumeFound(id, chat.id);
      if (folder !== undefined && folder !== chat.folder) {
        await tabMoveFolder(id, folder);
        setTree(setFolders(tree(), id, folder, []));
      }
    } catch (error) {
      // A project made for this chat goes with it, unless another tab joined it while this one waited.
      const left = closeTab(tree(), id);
      const empty = !left.tabs.some((t) => t.projectId === projectId);
      setTree(madeProject && empty ? removeProject(left, projectId) : left);
      throw error;
    }
    setRun(id, 1);
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
    prefills.forget(id);
    limits.forget(id);
    limitAsks.delete(id);
    lastState.delete(id);
    setLimitShown(limits.first());
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
      // Under a layer the keys stay with it, as a shortcut could stack a second one or close the tab it is about; its own toggle still works.
      const ownToggle =
        (shortcut === "shortcuts" && sheet()) ||
        (shortcut === "findChat" && finding());
      if (layerOpen() && !ownToggle) return;
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
      if (shortcut === "findChat") {
        if (!inSetup()) setFinding((open) => !open);
        return;
      }
      if (shortcut === "sidebar") {
        if (!inSetup()) toggleSidebar();
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
    // `claude --resume` in a shell tab named another account's chat; it opens as Find chat would open it.
    const elsewhere = listen<FoundChat>("chat-elsewhere", (event) => {
      const from = shown();
      openFound(event.payload).catch((error: unknown) => {
        if (from !== undefined)
          setAlert(from, `Couldn't open the chat: ${String(error)}`);
      });
    });
    onCleanup(() => {
      window.removeEventListener("keydown", onKey, true);
      for (const listening of [fromMenu, elsewhere]) {
        void listening.then((stop) => {
          stop();
        });
      }
    });
  });

  // A new tab in `folder` under the shown tab, on its account; only from a project tab.
  const openBeside = (
    choose: (programs: ProgramChoice[]) => ProgramChoice | undefined,
    folder: string,
    title: string,
    input?: string,
  ) => {
    const from = shownTab();
    const current = profiles();
    const program = choose(programList() ?? []);
    if (from?.projectId == null || !current || !program) return;
    const account =
      current.accounts.find((a) => a.id === from.launch.account?.id) ?? null;
    const tab = tabFor(program, { account, cwd: folder, input });
    if (!tab) return;
    const place = {
      projectId: from.projectId,
      parentId: from.parentId ?? from.id,
    };
    const [next, id] = addTab(tree(), place, tab);
    setTree(renameTab(next, id, title));
    select(id);
  };

  const shellOf = (programs: ProgramChoice[]) => {
    const current = profiles();
    return current && shellChoice(current, programs);
  };

  // Typed into a new shell beside the tab, never run: the user reads it and presses Enter.
  const createPr = (folder: string) => {
    openBeside(shellOf, folder, "Create PR", CREATE_PR);
  };

  // A worktree's own tab, named after its folder.
  const openTabIn = (folder: string, kind: "shell" | "claude") => {
    openBeside(
      kind === "shell"
        ? shellOf
        : (programs) => programs.find((p) => p.id === "claude"),
      folder,
      folderName(folder),
    );
  };

  // Each open pane's way to type into its program, for the panel's prefills.
  const typers = new Map<TabId, (text: string) => void>();
  const senders = new Map<TabId, Sender>();
  // A folder added to a running chat waits here until the chat is idle with nothing typed, then goes as `/add-dir` (DEC-002 addendum).
  const addDirs = new Map<TabId, string>();
  const sendAddDir = (id: TabId) => {
    const folder = addDirs.get(id);
    const sender = senders.get(id);
    if (folder === undefined || !sender || lastState.get(id) !== "idle") return;
    if (!sender.empty()) return;
    addDirs.delete(id);
    sender.send(addDirCommand(folder));
    setAlert(id, `Added ${folder} to the chat`);
  };
  // A started task's prompt, typed once its chat is ready.
  const prefills = new Prefills();
  // Each chat's limit popup, once per limit window, one shown at a time.
  const limits = new LimitQueue();
  const [limitShown, setLimitShown] = createSignal<LimitHit>();
  const queueLimit = (hit: LimitHit) => {
    if (tabById(hit.tab) && limits.hit(hit)) setLimitShown(limits.first());
  };
  // Each chat's latest state report, so an offers answer that arrives after a newer one is dropped.
  const limitAsks = new Map<TabId, number>();
  const limitDone = () => {
    limits.done();
    setLimitShown(limits.first());
  };
  // The limit clock's record, for a chat that hit its limit before any meter said so.
  const limitReported = (id: TabId, state: LiveState) => {
    const ask = (limitAsks.get(id) ?? 0) + 1;
    limitAsks.set(id, ask);
    if (state.state !== "limited") {
      limits.cleared(id);
      setLimitShown(limits.first());
      return;
    }
    const account = tabById(id)?.launch.account?.id;
    if (account === undefined) return;
    limitOffers(account).then(
      (offers) => {
        const current =
          limitAsks.get(id) === ask &&
          tabById(id)?.launch.account?.id === account;
        if (!current) return;
        queueLimit({
          tab: id,
          account,
          resetsAt: state.resetsAt,
          offers,
          source: "clock",
        });
      },
      (error: unknown) => {
        console.error("could not read where the chat can go:", error);
      },
    );
  };
  // A meter's 5-hour window at 100% is a hit too; its warning already names the tab and the offers.
  createEffect(
    on(
      () => live().warnings,
      (warnings) => {
        for (const w of warnings.filter((w) => w.used >= 100)) {
          queueLimit({
            tab: w.tab,
            account: w.account,
            resetsAt: w.resetsAt,
            offers: w.offers,
            source: "meter",
          });
        }
      },
    ),
  );
  // The low-limit popup over the shown tab, with the core's latest numbers; a full window has the limit popup instead.
  const warnShown = createMemo(() => {
    const id = shown();
    if (id === undefined || limitShown() !== undefined) return undefined;
    const waiting = warnWaiting().find((w) => w.tab === id);
    if (!waiting) return undefined;
    const now =
      live().warnings.find((w) => w.account === waiting.account) ?? waiting;
    return now.used < 100 && now.resetsAt > Date.now() / 1000 ? now : undefined;
  });
  // Dismissed for the rest of its window: its popup and its bar both go.
  const endWarning = (account: AccountId) => {
    warnDone(account);
    dismissWarning(account).catch((error: unknown) => {
      console.error("could not dismiss the warning:", error);
    });
  };
  // The tab whose Start a task dialog is open.
  const [startingTask, setStartingTask] = createSignal<TabId>();
  // A modal dialog other than the limit popup is open.
  const otherDialog = () => {
    return (
      editing() !== undefined ||
      startingTask() !== undefined ||
      moveFailed() !== undefined ||
      handoffFailed() !== undefined ||
      finding() ||
      serviceForm() !== undefined ||
      sheet()
    );
  };
  // Any layer over the window, menus included; window shortcuts wait for it so two never stack.
  const layerOpen = () =>
    otherDialog() ||
    pillMenu() !== undefined ||
    sidebarMenu() ||
    opening() !== undefined ||
    limitShown() !== undefined ||
    warnShown() !== undefined;
  // A Claude tab in a project with a planner above it, or that is one, can start a task.
  const canStartTask = (id: TabId) => {
    const tab = tabById(id);
    return (
      tab?.launch.program === "claude" && plannerFor(tree(), tab) !== undefined
    );
  };
  // The menu's Start a task, which closes the menu first; none for a tab that can't start one.
  const startTaskFrom = (id: TabId) =>
    canStartTask(id)
      ? () => {
          setPillMenu(undefined);
          setStartingTask(id);
        }
      : undefined;
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

  // A coder tab's handoff: checked when its chat writes or goes idle, never by reading the file's content.
  const handoffAsking = new Set<TabId>();
  const handoffAgain = new Set<TabId>();
  const handoffNotice = (id: TabId) => {
    const tab = tabById(id);
    if (!tab || shown() === id || !alertsSetting.on()) return;
    const project = tree().projects.find((p) => p.id === tab.projectId)?.name;
    sendWarning(
      {
        title: project === undefined ? tab.title : `${tab.title} · ${project}`,
        body: "Handoff ready",
      },
      id,
    );
  };
  const checkHandoff = (id: TabId) => {
    const handoff = tabById(id)?.handoff;
    if (handoff?.state !== "working") return;
    // One ask at a time per tab; a trigger meanwhile asks once more after it.
    if (handoffAsking.has(id)) {
      handoffAgain.add(id);
      return;
    }
    handoffAsking.add(id);
    const settled = () => {
      handoffAsking.delete(id);
      if (handoffAgain.delete(id)) checkHandoff(id);
    };
    handoffWritten(id, handoff.file, handoff.since).then(
      (written) => {
        const now = tabById(id)?.handoff;
        if (written && now?.state === "working" && now.file === handoff.file) {
          setTree(setHandoff(tree(), id, { ...now, state: "ready" }));
          handoffNotice(id);
        }
        settled();
      },
      (error: unknown) => {
        console.error("could not check the handoff file:", error);
        settled();
      },
    );
  };
  // Each tab's newest written file seen, so a live update that wrote nothing new asks nothing.
  const outputSeen = new Map<TabId, string>();
  createEffect(
    on(
      () => live().tabs,
      (tabs) => {
        for (const t of tabs) {
          const newest = t.outputs[0];
          if (!newest) continue;
          const mark = `${newest.path}|${String(newest.at)}`;
          if (outputSeen.get(t.tab) === mark) continue;
          outputSeen.set(t.tab, mark);
          checkHandoff(t.tab);
        }
      },
    ),
  );
  // A handoff written while Cerberus was closed is found once the saved tabs are in.
  createEffect(
    on(restored, (done) => {
      if (done) for (const t of untrack(tree).tabs) checkHandoff(t.id);
    }),
  );
  const [handoffFailed, setHandoffFailed] = createSignal<string>();
  // Each chat's last reported state, so text goes in now only to a chat that has drawn its input box.
  const lastState = new Map<TabId, LiveState["state"]>();
  // Typed in for the user to send; a planner not started or not ready yet gets it once its chat is idle.
  const typeForPlanner = (planner: TabId, text: string) => {
    const typer = typers.get(planner);
    const up = lastState.has(planner) && !ended().has(planner);
    if (typer && up) typer(text);
    else prefills.set(planner, text);
  };
  /** Tells the parent planner where the handoff is: typed in on its own account, the approval popup on another. */
  const sendHandoff = (id: TabId, file: string) => {
    const tab = tabById(id);
    const parentId = tab?.parentId ?? null;
    const planner = parentId === null ? undefined : tabById(parentId);
    if (!tab || !planner) return;
    const known = tab.handoff?.file === file ? tab.handoff : undefined;
    handoffMessage(tab.launch.spec.cwd ?? "", file, known?.taskId ?? null)
      .then(async (message) => {
        // Read again: the planner may have closed, or the coder moved, while the core looked for the PR.
        const coder = tabById(id);
        if (!coder) return;
        const target =
          coder.parentId === planner.id ? tabById(planner.id) : undefined;
        if (!target) {
          setHandoffFailed("The planner tab closed");
          return;
        }
        select(id);
        split.open(id, target.id);
        if (coder.launch.account?.id === target.launch.account?.id) {
          typeForPlanner(target.id, message.text);
        } else {
          await handoffOffer(id, target.id, message.text);
        }
        if (tabById(id)) {
          setTree(
            setHandoff(tree(), id, {
              taskId: message.taskId,
              file,
              since: known?.since ?? Date.now(),
              state: "sent",
            }),
          );
        }
      })
      .catch((error: unknown) => {
        setHandoffFailed(String(error));
      });
  };
  // A tab started by hand names its handoff here, starting at the newest Markdown its chat wrote.
  const chooseHandoff = (id: TabId) => {
    const tab = tabById(id);
    const outputs = live().tabs.find((t) => t.tab === id)?.outputs ?? [];
    const from =
      newestMarkdown(outputs) ??
      tab?.handoff?.file ??
      tab?.launch.spec.cwd ??
      undefined;
    pickHandoff(from).then(
      (file) => {
        if (file !== null) sendHandoff(id, file);
      },
      (error: unknown) => {
        setHandoffFailed(String(error));
      },
    );
  };
  /** What a coder tab's menu offers: its ready handoff, or a file to choose. */
  const handoffAction = (
    id: TabId,
  ): { label: string; run: () => void } | undefined => {
    const tab = tabById(id);
    const parentId = tab?.parentId ?? null;
    const parent = parentId === null ? undefined : tabById(parentId);
    if (tab?.launch.program !== "claude" || !parent) return undefined;
    const handoff = tab.handoff;
    if (handoff && handoff.state !== "working") {
      return {
        label:
          handoff.state === "sent"
            ? "Send to planner again"
            : "Send to planner",
        run: () => {
          sendHandoff(id, handoff.file);
        },
      };
    }
    return {
      label: "Send handoff…",
      run: () => {
        chooseHandoff(id);
      },
    };
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
    addDir: (id: TabId, folder: string) => {
      if (tabById(id)?.launch.program !== "claude" || !isRunning(id)) return;
      addDirs.set(id, folder);
      sendAddDir(id);
    },
    setAlert,
  };

  // Add to the tab: saved through Edit environment's own path, which types `/add-dir` into a running chat.
  const addFolder = (folder: string) => {
    const id = shown();
    const tab = id === undefined ? undefined : tabById(id);
    if (id === undefined || !tab || applying().has(id)) return;
    const dirs = tab.launch.spec.dirs ?? [];
    if (tab.launch.spec.cwd === folder || dirs.includes(folder)) return;
    saveEnvironment(id, {
      title: tab.title,
      cwd: tab.launch.spec.cwd,
      dirs: [...dirs, folder],
      env: [],
    }).catch((error: unknown) => {
      setAlert(id, `Could not add the folder: ${String(error)}`);
    });
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
            Start fresh to set up your accounts again. The old file is kept as{" "}
            <code>profiles.json.bad</code>.
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
            classList={{
              drawer: drawer(),
              "sidebar-rail": mode() === "rail" && !inSetup(),
              "panel-over": panelOver(),
            }}
            style={{
              "--sidebar-width": `${String(
                mode() === "rail" && !inSetup()
                  ? RAIL_WIDTH
                  : sidebarWidth.value(),
              )}px`,
            }}
          >
            <Show when={mode() !== "rail" || inSetup()}>
              <SidebarEdge
                width={sidebarWidth.value()}
                onWidth={sidebarWidth.drag}
                onDone={sidebarWidth.save}
              />
            </Show>
            <Sidebar
              locked={inSetup()}
              opened={revealed()}
              onCollapse={toggleSidebar}
              tree={tree()}
              profiles={loaded()}
              statuses={statuses}
              active={shown()}
              mark={(id) => markOf(board(), id)}
              running={isRunning}
              movingTo={(id) => moving().get(id)}
              ended={(id) => ended().has(id)}
              removed={onRemoved}
              onContinue={(id, account) => {
                void continueOn(id, account);
              }}
              handoffAction={handoffAction}
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
              onMenuOpen={setSidebarMenu}
              crosses={(from, to) => crossesOrganization(logins(), from, to.id)}
              onSearch={() => {
                setFinding(true);
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
            <Show when={mode() === "rail" && !inSetup()}>
              <SidebarRail
                projects={tree().projects}
                active={shownTab()?.projectId ?? undefined}
                waiting={(project) =>
                  tree().tabs.some(
                    (t) =>
                      t.projectId === project &&
                      markOf(board(), t.id)?.kind === "waiting",
                  )
                }
                onExpand={toggleSidebar}
                onOpenProject={openProject}
                extra={
                  <ServicesRailIcon
                    view={services.view()}
                    onOpen={() => {
                      setCollapsed(false);
                      if (services.view().folded) {
                        foldServices(false).then(services.set, onServiceError);
                      }
                    }}
                  />
                }
              />
            </Show>
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
                <Show
                  when={(() => {
                    const id = shown();
                    return id !== undefined &&
                      tabById(id)?.handoff?.state === "ready"
                      ? id
                      : undefined;
                  })()}
                >
                  {(id) => (
                    <button
                      type="button"
                      class="handoff-ready"
                      title="Tells the planner where the handoff is. You send it."
                      onClick={() => {
                        const file = tabById(id())?.handoff?.file;
                        if (file !== undefined) sendHandoff(id(), file);
                      }}
                    >
                      <span class="handoff-ready-state">Handoff ready · </span>
                      Send to planner
                    </button>
                  )}
                </Show>
                <Show when={messaging.note(shown())}>
                  {(note) => (
                    <span
                      class="messages-note"
                      role="status"
                      title={noteText(note()).title}
                    >
                      {noteText(note()).label}
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
                    title="Right panel"
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
                      Signing in to {tab().launch.account?.name}. Close this tab
                      when done.
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
                warnings={live().warnings.filter(
                  (w) => w.account !== warnShown()?.account,
                )}
                profiles={loaded()}
                tabTitle={(id) => tabById(id)?.title}
                onShow={(warning) => {
                  waitWarning(warning);
                  select(warning.tab);
                }}
                onDismiss={endWarning}
              />
              <Show when={handBarShown()} keyed>
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
                      class="secondary icon-only"
                      aria-label="Swap sides"
                      title="Swap sides"
                      onClick={() => {
                        split.swap();
                      }}
                    >
                      ⇄
                    </button>
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
                            when={!onRemoved(id) && runOf(id) !== 0}
                            fallback={
                              <Show
                                when={moving().get(id)}
                                fallback={
                                  <Show when={onRemoved(id)}>
                                    <RemovedNote
                                      name={
                                        tab().launch.account?.name ??
                                        "This account"
                                      }
                                      chat={tab().launch.program === "claude"}
                                      to={defaultTarget()}
                                      accounts={loaded().accounts}
                                      crosses={(to) =>
                                        crossesOrganization(
                                          logins(),
                                          tab().launch.account?.id,
                                          to.id,
                                        )
                                      }
                                      onMove={(to) => {
                                        moveRemoved(id, to);
                                      }}
                                    />
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
                              run={runOf(id)}
                              paused={moving().has(id)}
                              note={`Moving to ${moving().get(id) ?? ""}…`}
                              kind={tabKind(
                                tab().launch.program,
                                ended().has(id),
                              )}
                              imagePasteKey={
                                ended().has(id)
                                  ? null
                                  : tab().launch.imagePasteKey
                              }
                              input={tab().launch.input}
                              active={shown() === id || split.has(id)}
                              focused={shown() === id}
                              onState={(state) => {
                                prefills.reported(id, state, typers.get(id));
                                limitReported(id, state);
                                lastState.set(id, state.state);
                                if (state.state === "idle") {
                                  checkHandoff(id);
                                  sendAddDir(id);
                                }
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
                              onSender={(sender) => {
                                senders.set(id, sender);
                                return () => {
                                  if (senders.get(id) === sender)
                                    senders.delete(id);
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
                <Show when={fileShown()}>
                  {(file) => (
                    <div
                      class="tab-view shown service-view file-view"
                      classList={{ "split-right": beside() }}
                    >
                      <FilePane
                        tab={file().tab}
                        path={file().path}
                        outputs={
                          live().tabs.find((t) => t.tab === file().tab)
                            ?.outputs ?? []
                        }
                        onOpen={(path) => {
                          setFileOpen({ tab: file().tab, path });
                        }}
                        onClose={() => {
                          setFileOpen(undefined);
                        }}
                      />
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
                <Show when={ghost()}>
                  {(at) => (
                    <div
                      class="drag-ghost"
                      aria-hidden="true"
                      style={{
                        transform: `translate(${String(at().x + 14)}px, ${String(at().y + 10)}px)`,
                      }}
                    >
                      <ProgramIcon
                        program={
                          ended().has(at().id)
                            ? "shell"
                            : (tabById(at().id)?.launch.program ?? "")
                        }
                        size={13}
                      />
                      <span>{tabById(at().id)?.title}</span>
                    </div>
                  )}
                </Show>
                <Show when={dropping()}>
                  {(over) => (
                    <div
                      class="split-drop"
                      classList={{
                        over: over().side !== undefined,
                        left: over().side === "left",
                      }}
                    >
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
                          onMoveTabs={moveTabsOff}
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
                overlay={panelOver()}
                onClose={panel.toggle}
                profiles={loaded()}
                statuses={statuses}
                state={panel}
                canType={chatShown() !== undefined}
                onCreatePr={createPr}
                onOpenTab={openTabIn}
                onAddFolder={addFolder}
                tabTitle={(id) => tabById(id)?.title}
                onType={typeIntoChat}
                links={messaging.snapshot()}
                live={live()}
                openedFile={fileShown()?.path}
                onOpenFile={openFile}
                onEditEnvironment={() => {
                  const id = shown();
                  if (id !== undefined && !applying().has(id)) setEditing(id);
                }}
                onStartTask={(() => {
                  const id = shown();
                  return id !== undefined && canStartTask(id)
                    ? () => setStartingTask(id)
                    : undefined;
                })()}
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
            <Show
              when={(() => {
                const id = startingTask();
                const tab = id === undefined ? undefined : tabById(id);
                const project = tree().projects.find(
                  (p) => p.id === tab?.projectId,
                );
                return tab && project && { tab, project };
              })()}
            >
              {(open) => (
                <TaskDialog
                  tab={open().tab}
                  project={open().project}
                  profiles={loaded()}
                  onOpen={(choice) =>
                    openTask(
                      {
                        tree,
                        setTree,
                        profiles: loaded,
                        claude: () => installed(programList() ?? [], "claude"),
                        prefills,
                        select,
                      },
                      open().tab.id,
                      choice,
                    )
                  }
                  onTasks={(change) => {
                    setTree(
                      withProjectTasks(tree(), open().project.id, change),
                    );
                  }}
                  onClose={() => {
                    setStartingTask(undefined);
                  }}
                />
              )}
            </Show>
            <Show
              when={(() => {
                // Another dialog's keys would also take Escape and Tab, so the popup waits for it to close.
                if (otherDialog()) return undefined;
                const hit = limitShown();
                const title = hit && tabById(hit.tab)?.title;
                return hit && title !== undefined ? { hit, title } : undefined;
              })()}
            >
              {(shown) => (
                <LimitPopup
                  hit={shown().hit}
                  title={shown().title}
                  profiles={loaded()}
                  statuses={statuses}
                  onContinue={(account) => {
                    const id = shown().hit.tab;
                    limitDone();
                    void continueOn(id, account, true);
                  }}
                  onNotNow={limitDone}
                  crosses={(account) =>
                    crossesOrganization(
                      logins(),
                      shown().hit.account,
                      account.id,
                    )
                  }
                />
              )}
            </Show>
            <Show
              when={(() => {
                if (otherDialog()) return undefined;
                const warning = warnShown();
                const title = warning && tabById(warning.tab)?.title;
                return warning && title !== undefined
                  ? { warning, title }
                  : undefined;
              })()}
            >
              {(open) => (
                <LimitPopup
                  hit={{ ...open().warning, source: "meter" }}
                  used={open().warning.used}
                  title={open().title}
                  profiles={loaded()}
                  statuses={statuses}
                  onContinue={(account) => {
                    const { tab, account: from } = open().warning;
                    endWarning(from);
                    void continueOn(tab, account, true);
                  }}
                  onNotNow={() => {
                    warnDone(open().warning.account);
                  }}
                  crosses={(account) =>
                    crossesOrganization(
                      logins(),
                      open().warning.account,
                      account.id,
                    )
                  }
                />
              )}
            </Show>
            <Show when={handoffFailed()}>
              {(reason) => (
                <NoticePopup
                  title="Could not send the handoff"
                  message={`${reason()}.`}
                  onClose={() => {
                    setHandoffFailed(undefined);
                  }}
                />
              )}
            </Show>
            <Show when={moveFailed()}>
              {(failed) => (
                <NoticePopup
                  title={`Couldn't move to ${failed().account}`}
                  message={`${failed().reason}. The chat stays where it was.`}
                  onClose={() => {
                    setMoveFailed(undefined);
                  }}
                />
              )}
            </Show>
            <Show when={finding()}>
              <FindChat
                profiles={loaded()}
                onOpen={openFound}
                tabs={tree().tabs.map((tab) => ({
                  id: tab.id,
                  title: tab.title,
                  project:
                    tree().projects.find((p) => p.id === tab.projectId)?.name ??
                    "Setup",
                  folder: tab.launch.spec.cwd,
                  account: tab.launch.account ? tabName(tab.id).account : null,
                  accountId: tab.launch.account?.id ?? null,
                  // An ended chat's tab runs the shell, as its sidebar icon shows.
                  program: ended().has(tab.id) ? "shell" : tab.launch.program,
                }))}
                onSwitch={(id) => {
                  setFinding(false);
                  select(id);
                }}
                onClose={() => setFinding(false)}
              />
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
              senderGone={messaging.senderGone()}
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
                return open !== undefined && tabById(open.tab) !== undefined
                  ? open
                  : undefined;
              })()}
              keyed
            >
              {(open) => {
                const first = untrack(() => tabById(open.tab));
                if (first === undefined) return undefined;
                // Never throws: a read left over from the update that closes the menu sees the tab as it last was.
                let last = first;
                const tab = () => (last = tabById(open.tab) ?? last);
                return (
                  <TabMenu
                    title={tab().title}
                    chosen={open.chosen}
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
                    running={isRunning(open.tab)}
                    busy={moving().has(open.tab)}
                    // A call, not `a && b`: Solid wraps a logical prop in a memo, and one read from a click handler has no owner.
                    ended={endedHere(open.tab)}
                    at={open.at}
                    onContinue={(account) => {
                      setPillMenu(undefined);
                      void continueOn(open.tab, account);
                    }}
                    onResume={() => {
                      setPillMenu(undefined);
                      resume(open.tab);
                    }}
                    handoff={handoffAction(open.tab)}
                    onStartTask={startTaskFrom(open.tab)}
                    onAccounts={() => {
                      setPillMenu(undefined);
                      showPage("accounts");
                    }}
                    crosses={(account) =>
                      crossesOrganization(
                        logins(),
                        tab().launch.account?.id,
                        account.id,
                      )
                    }
                    onDismiss={() => {
                      setPillMenu(undefined);
                    }}
                  />
                );
              }}
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
