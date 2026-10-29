import {
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
  isNewer,
  loadProfiles,
  pickFolder,
  type Profiles,
  resetProfiles,
  statusText,
} from "./accounts";
import { AccountsScreen } from "./AccountsScreen";
import { CloseIcon, MenuIcon, PanelIcon } from "./icons";
import { shortcutFor } from "./keys";
import { type Launch, NewTab } from "./NewTab";
import { listPrograms } from "./pty";
import { AccountDot, Sidebar } from "./Sidebar";
import { createStatuses } from "./statuses";
import { type Tab, tabFor } from "./tab";
import { TerminalView } from "./Terminal";
import {
  addProject,
  addTab,
  closeTab,
  emptyTree,
  neighbour,
  planners,
  type ProjectId,
  renameTab,
  stepTab,
  type TabId,
  type Tree,
} from "./tree";
import { Walkthrough } from "./Walkthrough";

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
  const statuses = createStatuses();

  const [tree, setTree] = createSignal<Tree>(emptyTree());
  const [active, setActive] = createSignal<TabId>();
  const [opening, setOpening] = createSignal<Opening>();
  const [panelOpen, setPanelOpen] = createSignal(false);
  const [drawer, setDrawer] = createSignal(false);
  const [tabsClosed, setTabsClosed] = createSignal(0);

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

  const select = (id: TabId) => {
    setActive(id);
    showPage(undefined);
    setDrawer(false);
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
      <Match when={profiles()}>
        {(loaded) => (
          <div
            class="app"
            classList={{ drawer: drawer(), "panel-open": panelOpen() }}
          >
            <Sidebar
              tree={tree()}
              profiles={loaded()}
              statuses={statuses}
              active={shown()}
              onSelect={select}
              onClose={close}
              onRename={(id, title) => setTree(renameTab(tree(), id, title))}
              onNewTab={(projectId, anchor) => {
                setDrawer(false);
                setOpening({ projectId, anchor });
              }}
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
              <div class="stage">
                <For each={tabIds()}>
                  {(id) => (
                    <Show when={tabById(id)}>
                      {(tab) => (
                        <div
                          class="tab-view"
                          classList={{ shown: shown() === id }}
                        >
                          <TerminalView
                            spec={tab().launch.spec}
                            pastesImages={tab().launch.pastesImages}
                            input={tab().launch.input}
                            active={shown() === id}
                          />
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
                          hasProjects={tree().projects.length > 0}
                          onNewProject={newProject}
                        />
                      }
                    >
                      <Match when={page() === "walkthrough"}>
                        <Walkthrough
                          profiles={loaded()}
                          programs={programs.latest ?? []}
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
                          programs={programs.latest ?? []}
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
                      programs={programs.latest}
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

function Home(props: { hasProjects: boolean; onNewProject: () => void }) {
  return (
    <main class="home">
      <Show
        when={props.hasProjects}
        fallback={
          <>
            <h1>Add a project</h1>
            <p class="hint">
              A project is a folder. Its tabs start there, each under the
              account you pick.
            </p>
          </>
        }
      >
        <p class="hint">
          Pick a tab on the left, or press + beside a project for a new one.
        </p>
      </Show>
      <button
        type="button"
        class="primary"
        onClick={() => {
          props.onNewProject();
        }}
      >
        New project
      </button>
    </main>
  );
}
