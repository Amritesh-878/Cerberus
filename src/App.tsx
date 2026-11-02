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
  isNewer,
  loadProfiles,
  pickFolder,
  type Profiles,
  resetProfiles,
  setSidebarWidth,
  statusText,
} from "./accounts";
import { AccountsScreen } from "./AccountsScreen";
import { continueOnAccount } from "./carry";
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
import { AppIcon, CloseIcon, MenuIcon, PanelIcon } from "./icons";
import { keyHelp, shortcutFor } from "./keys";
import { type Launch, NewTab } from "./NewTab";
import { followOpenRequests } from "./open";
import { listPrograms } from "./pty";
import {
  fromSaved,
  loadTree,
  onWriteStatus,
  resumeChat,
  saveTree,
  toSaved,
} from "./saved";
import { AccountDot, Sidebar } from "./Sidebar";
import { createStatuses } from "./statuses";
import { type Tab, tabFor } from "./tab";
import { SidebarEdge } from "./SidebarEdge";
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
import { SIDEBAR_DEFAULT } from "./widths";

type Page = "walkthrough" | "accounts";

interface Opening {
  projectId: ProjectId;
  anchor: DOMRect | undefined;
}

export function App() {
  const [page, setPage] = createSignal<Page>();
  // A setup tab opened from a page shows over it, so the page keeps its place underneath.
  const [overPage, setOverPage] = createSignal(false);
  const showPage = (next: Page | undefined) => {
    setPage(next);
    setOverPage(false);
  };
  // Signals, not a resource: an errored resource keeps throwing after a fresh value is set.
  const [profiles, setProfiles] = createSignal<Profiles>();
  const [loadError, setLoadError] = createSignal<string>();
  const loaded = (fresh: Profiles) => {
    if (fresh.accounts.length === 0) showPage("walkthrough");
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
  const [panelOpen, setPanelOpen] = createSignal(false);
  // Held while dragging, so the edge follows the pointer before the saved value comes back.
  const [dragged, setDragged] = createSignal<number>();
  const sidebarWidth = () =>
    dragged() ?? profiles()?.sidebarWidth ?? SIDEBAR_DEFAULT;
  const [widthError, setWidthError] = createSignal<string>();
  // One save in flight; a newer width waits, and only the latest is sent, so saves land in order.
  let widthSaving = false;
  let widthWaiting: number | undefined;
  const sendWidth = (width: number) => {
    widthSaving = true;
    setSidebarWidth(width)
      .then(
        (saved) => {
          setWidthError(undefined);
          changed(saved);
        },
        (error: unknown) => {
          setWidthError(String(error));
          // A newer width still waiting gets its own try; with none, fall back to the saved one.
          if (widthWaiting === undefined) setDragged(undefined);
        },
      )
      .finally(() => {
        widthSaving = false;
        const next = widthWaiting;
        widthWaiting = undefined;
        if (next !== undefined) sendWidth(next);
      });
  };
  const saveWidth = () => {
    if (widthSaving) widthWaiting = sidebarWidth();
    else sendWidth(sidebarWidth());
  };
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
  const started = createMemo<ReadonlySet<TabId>>(
    (before) => startedTabs(before, shown(), tabIds()),
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
  const isRunning = (id: TabId) =>
    started().has(id) &&
    runOf(id) !== 0 &&
    !ended().has(id) &&
    markOf(board(), id)?.kind !== "done";

  const select = (id: TabId) => {
    setActive(id);
    showPage(undefined);
    setDrawer(false);
  };

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
    if (active() === id) {
      setActive(after);
      if (setup) setOverPage(false);
    }
    setTabsClosed((n) => n + 1);
    void recheckPrograms();
  };

  const forgetProject = (id: ProjectId) => {
    const next = removeProject(tree(), id);
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

  const newTabIn = (projectId: ProjectId) => {
    const button = document.querySelector(
      `[data-new-tab="${String(projectId)}"]`,
    );
    setDrawer(false);
    setOpening({ projectId, anchor: button?.getBoundingClientRect() });
  };

  const newProject = () => {
    pickFolder(profiles()?.startFolder ?? undefined).then(
      (folder) => {
        if (folder === null) return;
        const [next, id] = addProject(tree(), folder);
        setTree(next);
        showPage(undefined);
        setOpening({ projectId: id, anchor: undefined });
      },
      (error: unknown) => {
        console.error("could not open the folder picker:", error);
      },
    );
  };

  followOpenRequests({
    tree,
    shown,
    restored,
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
      event.preventDefault();
      event.stopPropagation();
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
    onCleanup(() => {
      window.removeEventListener("keydown", onKey, true);
    });
  });

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
            classList={{ drawer: drawer(), "panel-open": panelOpen() }}
            style={{ "--sidebar-width": `${String(sidebarWidth())}px` }}
          >
            <SidebarEdge
              width={sidebarWidth()}
              onWidth={setDragged}
              onDone={saveWidth}
            />
            <Sidebar
              tree={tree()}
              profiles={loaded()}
              statuses={statuses}
              active={shown()}
              mark={(id) => markOf(board(), id)}
              running={isRunning}
              movingTo={(id) => moving().get(id)}
              ended={(id) => ended().has(id)}
              onContinue={continueOn}
              onResume={resume}
              onSelect={select}
              onClose={close}
              onRename={(id, title) => setTree(renameTab(tree(), id, title))}
              onNewTab={(projectId, anchor) => {
                setDrawer(false);
                setOpening({ projectId, anchor });
              }}
              onRenameProject={(id, name) =>
                setTree(renameProject(tree(), id, name))
              }
              onRemoveProject={forgetProject}
              onNewProject={newProject}
              onAccounts={() => {
                showPage("accounts");
                setDrawer(false);
              }}
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
                <Show when={widthError()}>
                  {(error) => (
                    <span class="save-error" role="status" title={error()}>
                      Sidebar width not saved
                    </span>
                  )}
                </Show>
                <Show when={shownTab()?.launch.account}>
                  {(account) => (
                    <span
                      class="account-pill"
                      title={statusText(statuses.get(account().id))}
                    >
                      <AccountDot id={account().id} />
                      {loaded().accounts.find((a) => a.id === account().id)
                        ?.name ?? account().name}
                    </span>
                  )}
                </Show>
                <button
                  type="button"
                  class="icon-button"
                  aria-label="Side panel"
                  aria-pressed={panelOpen()}
                  title="Side panel"
                  onClick={() => setPanelOpen((open) => !open)}
                >
                  <PanelIcon />
                </button>
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
              <div class="stage">
                <For each={startedIds()}>
                  {(id) => (
                    <Show when={tabById(id)}>
                      {(tab) => (
                        <div
                          class="tab-view"
                          classList={{ shown: shown() === id }}
                        >
                          <Show
                            when={runOf(id)}
                            keyed
                            fallback={
                              <Show when={moving().get(id)}>
                                {(to) => (
                                  <p class="pane-note">Moving to {to()}…</p>
                                )}
                              </Show>
                            }
                          >
                            <TerminalView
                              tab={id}
                              spec={tab().launch.spec}
                              imagePasteKey={tab().launch.imagePasteKey}
                              input={tab().launch.input}
                              active={shown() === id}
                              onState={(state) =>
                                setBoard((b) => reported(b, id, state, shown()))
                              }
                              onExit={() => setBoard((b) => exited(b, id))}
                              onEnded={() => {
                                markEnded(id, true);
                                setBoard((b) => cleared(b, id));
                              }}
                            />
                          </Show>
                        </div>
                      )}
                    </Show>
                  )}
                </For>
                <Show when={page() !== undefined || shown() === undefined}>
                  <div class="page" hidden={shown() !== undefined}>
                    <Switch
                      fallback={
                        <Home
                          project={homeProject()?.name}
                          hasProjects={tree().projects.length > 0}
                          treeError={treeError()}
                          treeNotice={treeNotice()}
                          onNewTab={() => {
                            const project = homeProject();
                            if (project) newTabIn(project.id);
                          }}
                          onNewProject={newProject}
                        />
                      }
                    >
                      <Match when={page() === "walkthrough"}>
                        <Walkthrough
                          profiles={loaded()}
                          programs={programList() ?? []}
                          programsError={programsError()}
                          statuses={statuses}
                          onProfiles={changed}
                          onOpen={openSetupTab}
                          onRecheckPrograms={() => void recheckPrograms()}
                          onFinish={() => {
                            showPage(undefined);
                          }}
                        />
                      </Match>
                      <Match when={page() === "accounts"}>
                        <AccountsScreen
                          profiles={loaded()}
                          programs={programList() ?? []}
                          statuses={statuses}
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
            <Show when={panelOpen()}>
              <aside class="panel" aria-label="Side panel">
                <header class="panel-head">
                  Panel
                  <span class="spacer" />
                  <button
                    type="button"
                    class="icon-button"
                    aria-label="Close panel"
                    onClick={() => setPanelOpen(false)}
                  >
                    <CloseIcon />
                  </button>
                </header>
                <p class="panel-empty">Nothing here yet.</p>
              </aside>
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

function Home(props: {
  /** The project the last shown tab is in, where the main button opens a new tab. */
  project: string | undefined;
  hasProjects: boolean;
  treeError: string | undefined;
  treeNotice: string | undefined;
  onNewTab: () => void;
  onNewProject: () => void;
}) {
  return (
    <main class="home">
      <AppIcon />
      <h1>Cerberus</h1>
      <Show when={props.treeNotice}>
        {(notice) => <p class="hint">{notice()}</p>}
      </Show>
      <Show when={props.treeError}>
        {(error) => (
          <p class="error">
            {error()}. Tabs opened in this window will not be saved.
          </p>
        )}
      </Show>
      <Show
        when={props.hasProjects && props.project}
        fallback={
          <>
            <p class="home-lead">Add a project</p>
            <p class="hint">
              A project is a folder. Its tabs start there, each under the
              account you pick.
            </p>
            <div class="home-actions">
              <button
                type="button"
                class="primary"
                onClick={() => {
                  props.onNewProject();
                }}
              >
                New project
              </button>
            </div>
          </>
        }
      >
        {(project) => (
          <div class="home-actions">
            <button
              type="button"
              class="primary"
              title={`In ${project()}`}
              onClick={() => {
                props.onNewTab();
              }}
            >
              New Claude Code tab
            </button>
            <button
              type="button"
              class="secondary"
              onClick={() => {
                props.onNewProject();
              }}
            >
              New project
            </button>
          </div>
        )}
      </Show>
      <dl class="home-keys">
        <For each={keyHelp()}>
          {(help) => (
            <>
              <dt>{help.keys}</dt>
              <dd>{help.does}</dd>
            </>
          )}
        </For>
      </dl>
    </main>
  );
}
