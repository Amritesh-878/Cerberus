import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Index,
  Match,
  on,
  Show,
  Switch,
} from "solid-js";

import {
  accountColor,
  type AccountId,
  type Profiles,
  statusText,
} from "./accounts";
import { type Mark, resetText } from "./board";
import {
  ChevronIcon,
  ClockIcon,
  CloseIcon,
  FolderIcon,
  PeopleIcon,
  PlusIcon,
  ProgramIcon,
  SearchIcon,
} from "./icons";
import type { Statuses } from "./statuses";
import type { TabAccount } from "./tab";
import {
  groups,
  type Group,
  type ProjectId,
  reuseRows,
  type Row,
  type TabId,
  type Tree,
  type TreeTab,
} from "./tree";

export function Sidebar(props: {
  tree: Tree;
  profiles: Profiles;
  statuses: Statuses;
  active: TabId | undefined;
  mark: (id: TabId) => Mark | undefined;
  onSelect: (id: TabId) => void;
  onClose: (id: TabId) => void;
  onRename: (id: TabId, title: string) => void;
  onNewTab: (projectId: ProjectId, anchor: DOMRect) => void;
  onRenameProject: (id: ProjectId, name: string) => void;
  onRemoveProject: (id: ProjectId) => void;
  onNewProject: () => void;
  onAccounts: () => void;
}) {
  const [search, setSearch] = createSignal("");
  const [renaming, setRenaming] = createSignal<TabId>();
  const [folded, setFolded] = createSignal<ReadonlySet<ProjectId>>(new Set());

  // The account's current name, so a rename in Accounts reaches tabs already open.
  const accountOf = (tab: TreeTab): TabAccount | null => {
    const launched = tab.launch.account;
    if (!launched) return null;
    return (
      props.profiles.accounts.find((a) => a.id === launched.id) ?? launched
    );
  };
  // Rows kept across rebuilds, so a late save or a search does not reset a rename in progress.
  const listed = createMemo<Group[]>(
    (previous) =>
      reuseRows(
        previous,
        groups(props.tree, search(), (tab) => accountOf(tab)?.name ?? ""),
      ),
    [],
  );
  const isFolded = (group: Group) =>
    group.project !== null &&
    search().trim() === "" &&
    folded().has(group.project.id);
  const toggle = (id: ProjectId) => {
    const next = new Set(folded());
    if (!next.delete(id)) next.add(id);
    setFolded(next);
  };

  return (
    <aside class="sidebar">
      <div class="sidebar-top">
        <label class="search">
          <SearchIcon />
          <input
            type="search"
            placeholder="Search tabs"
            aria-label="Search tabs"
            value={search()}
            onInput={(event) => setSearch(event.currentTarget.value)}
          />
        </label>
      </div>
      <nav class="tree" aria-label="Tabs">
        <Index each={listed()}>
          {(group) => (
            <section>
              <ProjectHeader
                group={group()}
                folded={isFolded(group())}
                tabCount={
                  props.tree.tabs.filter(
                    (t) =>
                      t.projectId !== null &&
                      t.projectId === group().project?.id,
                  ).length
                }
                onToggle={() => {
                  const project = group().project;
                  if (project) toggle(project.id);
                }}
                onNewTab={props.onNewTab}
                onRename={props.onRenameProject}
                onRemove={props.onRemoveProject}
              />
              <Show when={!isFolded(group())}>
                <For each={group().rows}>
                  {(row) => (
                    <TabRow
                      row={row}
                      account={accountOf(row.tab)}
                      status={(id) => statusText(props.statuses.get(id))}
                      active={props.active === row.tab.id}
                      mark={props.mark(row.tab.id)}
                      renaming={renaming() === row.tab.id}
                      onSelect={() => {
                        props.onSelect(row.tab.id);
                      }}
                      onClose={() => {
                        props.onClose(row.tab.id);
                      }}
                      onStartRename={() => setRenaming(row.tab.id)}
                      onRenamed={(title) => {
                        setRenaming(undefined);
                        if (title !== undefined)
                          props.onRename(row.tab.id, title);
                      }}
                    />
                  )}
                </For>
              </Show>
            </section>
          )}
        </Index>
        <Show when={search().trim() !== "" && listed().length === 0}>
          <p class="hint tree-empty">No tabs match.</p>
        </Show>
      </nav>
      <footer class="sidebar-foot">
        <button
          type="button"
          class="foot-button"
          onClick={() => {
            props.onNewProject();
          }}
        >
          <FolderIcon /> New project
        </button>
        <button
          type="button"
          class="icon-button"
          aria-label="Accounts"
          title="Accounts"
          onClick={() => {
            props.onAccounts();
          }}
        >
          <PeopleIcon />
        </button>
      </footer>
    </aside>
  );
}

function TabRow(props: {
  row: Row;
  account: TabAccount | null;
  status: (id: AccountId) => string;
  active: boolean;
  mark: Mark | undefined;
  renaming: boolean;
  onSelect: () => void;
  onClose: () => void;
  onStartRename: () => void;
  onRenamed: (title: string | undefined) => void;
}) {
  const tab = () => props.row.tab;
  const icon = () => (
    <Show
      when={!props.row.sub}
      fallback={<ProgramIcon program={tab().launch.program} size={13} />}
    >
      <span class="badge">
        <ProgramIcon program={tab().launch.program} size={15} />
      </span>
    </Show>
  );

  return (
    <div
      class="tab-row"
      classList={{
        sub: props.row.sub,
        last: props.row.last,
        active: props.active,
        done: props.mark?.kind === "done",
        waiting: props.mark?.kind === "waiting",
      }}
    >
      <Show
        when={props.renaming}
        fallback={
          <button
            type="button"
            class="tab-select"
            aria-current={props.active ? "page" : undefined}
            title="Double-click or F2 to rename"
            onClick={() => {
              props.onSelect();
            }}
            onDblClick={() => {
              props.onStartRename();
            }}
            onKeyDown={(event) => {
              if (event.key === "F2") props.onStartRename();
            }}
          >
            {icon()}
            <span class="tab-text">
              <span class="tab-title">{tab().title}</span>
            </span>
            <StateMark mark={props.mark} />
            <Show when={props.account}>
              {(account) => (
                <span
                  title={`${account().name} · ${props.status(account().id)}`}
                >
                  <AccountDot id={account().id} />
                </span>
              )}
            </Show>
          </button>
        }
      >
        <div class="tab-select">
          {icon()}
          <span class="tab-text">
            <RenameField
              title={tab().title}
              label="Tab name"
              onDone={props.onRenamed}
            />
          </span>
        </div>
      </Show>
      <button
        type="button"
        class="icon-button close"
        aria-label={`Close ${tab().title}`}
        title="Close (Ctrl+Shift+W)"
        onClick={() => {
          props.onClose();
        }}
      >
        <CloseIcon />
      </button>
    </div>
  );
}

function StateMark(props: { mark: Mark | undefined }) {
  return (
    <Switch>
      <Match when={props.mark?.kind === "working"}>
        <span
          class="mark working"
          role="img"
          aria-label="Working"
          title="Working"
        />
      </Match>
      <Match when={props.mark?.kind === "waiting"}>
        <span
          class="mark waiting"
          role="img"
          aria-label="Waiting on you"
          title="Waiting on you"
        />
      </Match>
      <Match when={props.mark?.kind === "limited" && props.mark}>
        {(mark) => {
          const at = () => {
            const { resetsAt } = mark();
            return resetsAt === null ? undefined : resetText(resetsAt);
          };
          return (
            <span
              class="mark limited"
              title={
                at() === undefined
                  ? "Usage limit reached"
                  : `Usage limit, resets ${at() ?? ""}`
              }
            >
              <ClockIcon />
              {at()}
            </span>
          );
        }}
      </Match>
    </Switch>
  );
}

export function AccountDot(props: { id: AccountId }) {
  return <span class="dot" style={{ background: accountColor(props.id) }} />;
}

function ProjectHeader(props: {
  group: Group;
  folded: boolean;
  tabCount: number;
  onToggle: () => void;
  onNewTab: (id: ProjectId, anchor: DOMRect) => void;
  onRename: (id: ProjectId, name: string) => void;
  onRemove: (id: ProjectId) => void;
}) {
  const [renaming, setRenaming] = createSignal(false);
  const [confirming, setConfirming] = createSignal(false);
  // Sections are kept by position, so another project landing here must not inherit an open edit.
  createEffect(
    on(
      () => props.group.project?.id,
      () => {
        setRenaming(false);
        setConfirming(false);
      },
      { defer: true },
    ),
  );
  const tabs = () =>
    `${String(props.tabCount)} tab${props.tabCount === 1 ? "" : "s"}`;

  return (
    <>
      <div class="project">
        <Show
          when={renaming() && props.group.project}
          fallback={
            <button
              type="button"
              class="project-name"
              classList={{ folded: props.folded }}
              aria-expanded={!props.folded}
              title={
                props.group.project
                  ? `${props.group.project.folder}\nDouble-click or F2 to rename`
                  : undefined
              }
              onClick={() => {
                props.onToggle();
              }}
              onDblClick={() => {
                if (props.group.project) setRenaming(true);
              }}
              onKeyDown={(event) => {
                if (event.key === "F2" && props.group.project)
                  setRenaming(true);
              }}
            >
              <ChevronIcon />
              <span>{props.group.project?.name ?? "Setup"}</span>
            </button>
          }
        >
          {(project) => (
            <div class="project-name">
              <ChevronIcon />
              <RenameField
                title={project().name}
                label="Project name"
                onDone={(name) => {
                  const id = project().id;
                  setRenaming(false);
                  if (name !== undefined) props.onRename(id, name);
                }}
              />
            </div>
          )}
        </Show>
        <Show when={props.group.project}>
          {(project) => (
            <>
              <button
                type="button"
                class="icon-button add"
                data-new-tab={project().id}
                aria-label={`New tab in ${project().name}`}
                title="New tab (Ctrl+Shift+T)"
                onClick={(event) => {
                  props.onNewTab(
                    project().id,
                    event.currentTarget.getBoundingClientRect(),
                  );
                }}
              >
                <PlusIcon />
              </button>
              <button
                type="button"
                class="icon-button remove"
                aria-label={`Remove ${project().name}`}
                title="Remove project (the folder stays)"
                onClick={() => {
                  if (props.tabCount > 0) setConfirming(true);
                  else props.onRemove(project().id);
                }}
              >
                <CloseIcon />
              </button>
            </>
          )}
        </Show>
      </div>
      <Show when={confirming() && props.group.project}>
        {(project) => (
          <div class="confirm" role="alert">
            <p>
              Close {tabs()} and remove {project().name}? The folder stays on
              disk.
            </p>
            <div class="confirm-actions">
              <button
                type="button"
                class="danger"
                onClick={() => {
                  const id = project().id;
                  setConfirming(false);
                  props.onRemove(id);
                }}
              >
                Remove
              </button>
              <button type="button" onClick={() => setConfirming(false)}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </Show>
    </>
  );
}

function RenameField(props: {
  title: string;
  label: string;
  onDone: (title: string | undefined) => void;
}) {
  let done = false;
  const finish = (title: string | undefined) => {
    if (done) return;
    done = true;
    props.onDone(title);
  };

  return (
    <input
      class="rename"
      value={props.title}
      aria-label={props.label}
      ref={(el) => {
        queueMicrotask(() => {
          el.focus();
          el.select();
        });
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") finish(event.currentTarget.value);
        if (event.key === "Escape") finish(undefined);
      }}
      onBlur={(event) => {
        finish(event.currentTarget.value);
      }}
    />
  );
}
