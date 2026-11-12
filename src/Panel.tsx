import { createSignal, For, Match, Show, Switch } from "solid-js";

import { type AccountId, type Profiles, savedColor } from "./accounts";
import { Card, Note, Row } from "./Cards";
import {
  AccountDot,
  BranchIcon,
  CloseIcon,
  CodeIcon,
  DiffIcon,
  FolderIcon,
  GearIcon,
  PlugIcon,
  SkillIcon,
  StarIcon,
} from "./icons";
import {
  type Changes,
  connectorScope,
  favouritesFirst,
  openFolder,
  openPr,
  openVsCode,
  type PrRow,
  prLink,
  prReady,
  type Skill,
  skillCommand,
  skillSource,
} from "./panelData";
import {
  createPanelFeed,
  type FolderChanges,
  type Loaded,
  POLL_MS,
  type RepoPrs,
  type Target,
} from "./panelFeed";
import type { PanelState } from "./panelPrefs";
import { type PrActions, PrList } from "./PrList";
import type { LiveView } from "./live";
import { ContextCard, UsageCard } from "./LiveCards";
import { PanelEdge } from "./SidebarEdge";
import type { Statuses } from "./statuses";
import { folderName, type TabId, type TreeTab } from "./tree";
import { OutputsCard, TasksCard } from "./WorkCards";

/** The value once loaded, so a card can show a note while loading or on an error. */
function loadedValue<T>(loaded: Loaded<T>): T | undefined {
  return loaded !== undefined && "value" in loaded ? loaded.value : undefined;
}

function loadedError<T>(loaded: Loaded<T>): string | undefined {
  return loaded !== undefined && "error" in loaded ? loaded.error : undefined;
}

function branchText(changes: Changes | undefined): string | undefined {
  if (changes?.kind !== "repo") return undefined;
  return changes.branch ?? `detached at ${changes.commit ?? "HEAD"}`;
}

/** The shown tab's account, folder, changes, connectors and skills; mounted only while open. */
export function Panel(props: {
  tab: TreeTab | undefined;
  profiles: Profiles;
  statuses: Statuses;
  state: PanelState;
  /** Whether the shown tab is a running chat that a shift-clicked skill can be typed into. */
  canType: boolean;
  onCreatePr: (folder: string) => void;
  /** Types into the shown chat, never with Enter. */
  onType: (text: string) => void;
  /** The bridge's latest report, which changes as chats link and unlink. */
  links: unknown;
  live: LiveView;
  /** The file the file pane shows, if any. */
  openedFile?: string;
  onOpenFile: (tab: TabId, path: string) => void;
  onEditEnvironment: () => void;
  /** Over the chat, since beside it the chat would be too narrow; it then has its own close button. */
  overlay?: boolean;
  onClose?: () => void;
}) {
  const target = (): Target | undefined => {
    const tab = props.tab;
    if (tab === undefined) return undefined;
    return {
      tab: tab.id,
      folder: tab.launch.spec.cwd,
      account: tab.launch.account?.id ?? null,
      dirs: tab.launch.spec.dirs ?? [],
    };
  };
  const feed = createPanelFeed(target, POLL_MS, () => props.links);
  const [failed, setFailed] = createSignal<string>();
  const act = (doing: Promise<void>) => {
    doing.then(
      () => {
        setFailed(undefined);
      },
      (error: unknown) => setFailed(`Could not open it: ${String(error)}`),
    );
  };
  const folders = () => loadedValue(feed.folders()) ?? [];
  const changesOf = (folder: string) =>
    folders().find((f) => f.folder === folder)?.changes;
  const changes = () => folders()[0]?.changes;
  const prActions: PrActions = {
    open: (folder, pr) => {
      act(openPr(folder, pr.number));
    },
    copy: (folder, pr) => {
      act(
        prLink(folder, pr.number).then((link) =>
          navigator.clipboard.writeText(link),
        ),
      );
    },
    dismiss: (pr) => {
      props.state.dismiss(pr.key, pr.updated);
    },
    ready: (folder, pr) => {
      prReady(folder, pr.number).then(
        () => {
          setFailed(undefined);
          feed.rereadPrs();
        },
        (error: unknown) => setFailed(String(error)),
      );
    },
  };
  const account = () => props.tab?.launch.account ?? null;
  const accountName = (id: number, fallback: string) =>
    props.profiles.accounts.find((a) => a.id === id)?.name ?? fallback;
  const connectors = () => loadedValue(feed.connectors());
  const skills = () => {
    const found = loadedValue(feed.skills());
    const id = account()?.id;
    if (found === undefined || id === undefined) return found;
    return favouritesFirst(found, props.state.favourites(id));
  };

  return (
    <aside
      class="panel"
      aria-label="Tab details"
      style={{ "--panel-width": `${String(props.state.width())}px` }}
    >
      <PanelEdge
        width={props.state.width()}
        onWidth={(width) => {
          props.state.drag(width);
        }}
        onDone={() => {
          props.state.save();
        }}
      />
      <Show when={props.overlay}>
        <div class="panel-over-bar">
          <span>Tab details</span>
          <button
            type="button"
            class="icon-button"
            aria-label="Close the panel"
            title="Close the panel"
            onClick={() => {
              props.onClose?.();
            }}
          >
            <CloseIcon />
          </button>
        </div>
      </Show>
      <div class="panel-cards">
        <Show
          when={props.tab}
          fallback={
            <p class="panel-empty">Open a tab to see its details here.</p>
          }
        >
          <Card
            id="environment"
            title="Environment"
            state={props.state}
            action={
              <Show when={props.tab?.projectId != null}>
                <button
                  type="button"
                  class="card-action"
                  aria-label="Edit environment"
                  title="Edit environment: name, folders and variables"
                  onClick={() => {
                    props.onEditEnvironment();
                  }}
                >
                  <GearIcon />
                </button>
              </Show>
            }
          >
            <Show
              when={account()}
              fallback={
                <Row icon={<span class="dot none" />} text="No account" />
              }
            >
              {(tabAccount) => {
                const status = () => props.statuses.get(tabAccount().id);
                return (
                  <Row
                    icon={
                      <AccountDot
                        id={tabAccount().id}
                        color={savedColor(props.profiles, tabAccount().id)}
                      />
                    }
                    text={accountName(tabAccount().id, tabAccount().name)}
                    detail={
                      status()?.state === "loggedIn"
                        ? (status()?.email ?? null)
                        : (status()?.summary ?? "checking…")
                    }
                    meta={status()?.plan ?? undefined}
                  />
                );
              }}
            </Show>
            <Show
              when={props.tab?.launch.spec.cwd}
              fallback={<Row icon={<FolderIcon />} text="Home folder" />}
            >
              {(folder) => (
                <button
                  type="button"
                  class="card-row action"
                  title={`${folder()}\nOpen in the file manager`}
                  onClick={() => {
                    act(openFolder(folder()));
                  }}
                >
                  <span class="row-icon">
                    <FolderIcon />
                  </span>
                  <span class="row-text">
                    <span class="row-main">{folderName(folder())}</span>
                  </span>
                </button>
              )}
            </Show>
            <For each={props.tab?.launch.spec.dirs ?? []}>
              {(dir) => (
                <button
                  type="button"
                  class="card-row action"
                  title={`${dir}\nOpen in the file manager`}
                  onClick={() => {
                    act(openFolder(dir));
                  }}
                >
                  <span class="row-icon">
                    <FolderIcon />
                  </span>
                  <span class="row-text">
                    <span class="row-main">{folderName(dir)}</span>
                    <Show when={branchText(changesOf(dir))}>
                      {(branch) => <span class="row-detail">{branch()}</span>}
                    </Show>
                  </span>
                </button>
              )}
            </For>
            <Show when={branchText(changes())}>
              {(branch) => <Row icon={<BranchIcon />} text={branch()} />}
            </Show>
          </Card>
          <Show when={props.tab?.launch.program === "claude"}>
            <ContextCard
              live={props.live.tabs.find((t) => t.tab === props.tab?.id)}
              liveOn={props.profiles.live}
              canType={props.canType}
              state={props.state}
              onCompact={() => {
                props.onType("/compact");
              }}
            />
            <OutputsCard
              live={props.live.tabs.find((t) => t.tab === props.tab?.id)}
              liveOn={props.profiles.live}
              state={props.state}
              opened={props.openedFile}
              onOpenFile={props.onOpenFile}
            />
            <TasksCard
              canType={props.canType}
              onType={props.onType}
              live={props.live.tabs.find((t) => t.tab === props.tab?.id)}
              liveOn={props.profiles.live}
              state={props.state}
            />
          </Show>
          <UsageCard
            profiles={props.profiles}
            meters={props.live.meters}
            state={props.state}
          />
          <Card id="changes" title="Changes" state={props.state}>
            <ChangesBody
              folder={props.tab?.launch.spec.cwd ?? null}
              folders={feed.folders()}
              prs={loadedValue(feed.prs()) ?? []}
              isDismissed={(pr) => props.state.isDismissed(pr.key, pr.updated)}
              actions={prActions}
              onCreatePr={props.onCreatePr}
              onOpenVsCode={(folder) => {
                act(openVsCode(folder));
              }}
            />
          </Card>
          <Card
            id="skills"
            title="Skills"
            meta={skills()?.length ? String(skills()?.length) : undefined}
            state={props.state}
          >
            <Show
              when={skills()}
              fallback={<Note>{loadedError(feed.skills()) ?? "Reading…"}</Note>}
            >
              {(found) => (
                <For each={found()} fallback={<Note>No skills</Note>}>
                  {(skill) => (
                    <SkillRow
                      skill={skill}
                      account={account()?.id}
                      state={props.state}
                      canType={props.canType}
                      onType={props.onType}
                    />
                  )}
                </For>
              )}
            </Show>
          </Card>
          <Card
            id="connectors"
            title="Connectors"
            subtitle="MCP servers this chat loads"
            meta={
              connectors()?.servers.length
                ? `${String(connectors()?.servers.length)} configured`
                : undefined
            }
            state={props.state}
          >
            <Show
              when={connectors()}
              fallback={
                <Note>{loadedError(feed.connectors()) ?? "Reading…"}</Note>
              }
            >
              {(found) => (
                <>
                  <For
                    each={found().servers}
                    fallback={<Note>None configured</Note>}
                  >
                    {(server) => (
                      <Row
                        icon={<PlugIcon />}
                        text={server.name}
                        meta={`${server.kind} · ${connectorScope(server.scope)}`}
                      />
                    )}
                  </For>
                  <For each={found().unreadable}>
                    {(file) => <Note>Could not read {file}</Note>}
                  </For>
                  <p class="card-note faint">
                    Connectors added on claude.ai are not listed here
                  </p>
                </>
              )}
            </Show>
          </Card>
          <Show when={failed()}>
            {(message) => <p class="panel-error">{message()}</p>}
          </Show>
        </Show>
      </div>
    </aside>
  );
}

function skillTitle(skill: Skill, canType: boolean): string {
  const command = skillCommand(skill.name);
  const lines = [
    skill.description,
    skill.uses > 0
      ? `Used ${String(skill.uses)} time${skill.uses === 1 ? "" : "s"}`
      : null,
    canType
      ? `Shift-click or Shift+Enter types ${command} into the chat, for you to send`
      : `Shift-click types ${command} in Claude Code tabs`,
  ];
  return lines.filter((line) => line !== null).join("\n");
}

function SkillRow(props: {
  skill: Skill;
  account: AccountId | undefined;
  state: PanelState;
  canType: boolean;
  onType: (text: string) => void;
}) {
  const starred = () =>
    props.account !== undefined &&
    props.state.favourites(props.account).includes(props.skill.name);
  const insert = () => {
    if (props.canType) props.onType(skillCommand(props.skill.name));
  };
  return (
    <div
      class="card-row skill-row"
      title={skillTitle(props.skill, props.canType)}
    >
      <button
        type="button"
        class="skill-main"
        // Keeps a shift-click from selecting text across the rows.
        onMouseDown={(event) => {
          if (event.shiftKey) event.preventDefault();
        }}
        // A plain click does nothing, so only Shift with a click or Enter types.
        onClick={(event) => {
          if (event.shiftKey) insert();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && event.shiftKey) {
            event.preventDefault();
            insert();
          }
        }}
      >
        <span class="row-icon">
          <SkillIcon />
        </span>
        <span class="row-text">
          <span class="row-main">{props.skill.name}</span>
          <Show
            when={
              skillSource(props.skill.scope) !== null ||
              props.skill.description !== null
            }
          >
            <span class="row-detail">
              <Show when={skillSource(props.skill.scope)}>
                {(source) => <span class="row-tag">{source()}</span>}
              </Show>
              {props.skill.description}
            </span>
          </Show>
        </span>
      </button>
      {/* An id, which may be 0, so the check is against undefined rather than truthiness. */}
      <Show when={props.account !== undefined}>
        <button
          type="button"
          class="star"
          aria-label={`Favourite ${props.skill.name}`}
          aria-pressed={starred()}
          title={starred() ? "Unpin from the top" : "Pin to the top"}
          onClick={() => {
            if (props.account !== undefined) {
              props.state.toggleFavourite(props.account, props.skill.name);
            }
          }}
        >
          <StarIcon />
        </button>
      </Show>
    </div>
  );
}

type RepoChanges = Extract<Changes, { kind: "repo" }>;

function RepoRow(props: {
  folder: string;
  repo: RepoChanges;
  onCreatePr: (folder: string) => void;
  onOpenVsCode: (folder: string) => void;
}) {
  return (
    <Row
      icon={<DiffIcon />}
      text={props.repo.name}
      title={
        props.repo.base === null
          ? "Uncommitted work"
          : `Against ${props.repo.base}, uncommitted work included`
      }
      meta={
        <>
          <span class="added">+{props.repo.added}</span>
          <span class="removed">−{props.repo.removed}</span>
          <button
            type="button"
            class="row-button"
            disabled={!props.repo.gh || props.repo.branch === null}
            title={
              props.repo.gh
                ? "Opens a shell with gh pr create typed, for you to send"
                : "Install the GitHub CLI (gh) to create a PR from here"
            }
            onClick={() => {
              props.onCreatePr(props.folder);
            }}
          >
            Create PR
          </button>
          <Show when={props.repo.vscode}>
            <button
              type="button"
              class="row-button icon"
              aria-label="Open in VS Code"
              title="Open in VS Code, whose Source Control view shows the diff"
              onClick={() => {
                props.onOpenVsCode(props.folder);
              }}
            >
              <CodeIcon />
            </button>
          </Show>
        </>
      }
    />
  );
}

/** One row per repo among the tab's folders, the main folder first, each with its PRs below it. */
function ChangesBody(props: {
  folder: string | null;
  folders: Loaded<FolderChanges[]>;
  prs: RepoPrs[];
  isDismissed: (pr: PrRow) => boolean;
  actions: PrActions;
  onCreatePr: (folder: string) => void;
  onOpenVsCode: (folder: string) => void;
}) {
  const main = () => loadedValue(props.folders)?.[0];
  // Folders in one repo show its row once, under the first of them.
  const repos = () => {
    const seen = new Map<string, { folder: string; repo: RepoChanges }>();
    for (const f of loadedValue(props.folders) ?? []) {
      const changes = f.changes;
      if (changes?.kind === "repo" && !seen.has(changes.root)) {
        seen.set(changes.root, { folder: f.folder, repo: changes });
      }
    }
    return seen;
  };
  // Keyed by root, so each poll's fresh objects keep the rows, and an open PR menu, in place.
  const roots = () => [...repos().keys()];
  return (
    <Switch fallback={<Note>Reading…</Note>}>
      <Match when={props.folder === null}>
        <Note>No project folder</Note>
      </Match>
      <Match when={loadedError(props.folders)}>
        {(error) => <Note>{error()}</Note>}
      </Match>
      <Match when={loadedValue(props.folders)}>
        {/* Each folder's failure stays beside it, so the other repos still show. */}
        <For each={(loadedValue(props.folders) ?? []).filter((f) => f.error)}>
          {(failed) => (
            <Note>
              {folderName(failed.folder)}: {failed.error}
            </Note>
          )}
        </For>
        <Show when={main()?.changes?.kind === "notRepo"}>
          <Note>Not a git repository</Note>
        </Show>
        <Show when={main()?.changes?.kind === "noGit"}>
          <Note>Git is not installed</Note>
        </Show>
        <For each={roots()}>
          {(root) => (
            <>
              <Show when={repos().get(root)}>
                {(found) => (
                  <RepoRow
                    folder={found().folder}
                    repo={found().repo}
                    onCreatePr={props.onCreatePr}
                    onOpenVsCode={props.onOpenVsCode}
                  />
                )}
              </Show>
              <Show when={props.prs.find((r) => r.root === root)}>
                {(repoPrs) => (
                  <PrList
                    repo={repoPrs()}
                    isDismissed={props.isDismissed}
                    actions={props.actions}
                  />
                )}
              </Show>
            </>
          )}
        </For>
      </Match>
    </Switch>
  );
}
