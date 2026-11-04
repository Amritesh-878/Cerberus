import { createSignal, For, type JSX, Match, Show, Switch } from "solid-js";

import { type PanelCard, type Profiles, savedColor } from "./accounts";
import {
  AccountDot,
  BranchIcon,
  ChevronIcon,
  DiffIcon,
  FolderIcon,
  PlugIcon,
  PrIcon,
  SkillIcon,
} from "./icons";
import { type Changes, openFolder, openPr } from "./panelData";
import { createPanelFeed, type Loaded, type Target } from "./panelFeed";
import type { PanelState } from "./panelPrefs";
import { PanelEdge } from "./SidebarEdge";
import type { Statuses } from "./statuses";
import { folderName, type TreeTab } from "./tree";

function Card(props: {
  id: PanelCard;
  title: string;
  meta?: string;
  state: PanelState;
  children: JSX.Element;
}) {
  const open = () => !props.state.collapsed(props.id);
  return (
    <section class="card" classList={{ folded: !open() }}>
      <h2 class="card-head">
        <button
          type="button"
          class="card-title"
          aria-expanded={open()}
          onClick={() => {
            props.state.toggleCard(props.id);
          }}
        >
          {props.title}
          <ChevronIcon />
        </button>
        <Show when={props.meta}>
          <span class="card-meta">{props.meta}</span>
        </Show>
      </h2>
      <Show when={open()}>
        <div class="card-body">{props.children}</div>
      </Show>
    </section>
  );
}

function Row(props: {
  icon: JSX.Element;
  text: string;
  detail?: string | null;
  meta?: JSX.Element;
  title?: string;
}) {
  return (
    <div class="card-row" title={props.title}>
      <span class="row-icon">{props.icon}</span>
      <span class="row-text">
        <span class="row-main">{props.text}</span>
        <Show when={props.detail}>
          <span class="row-detail">{props.detail}</span>
        </Show>
      </span>
      <Show when={props.meta}>
        <span class="row-meta">{props.meta}</span>
      </Show>
    </div>
  );
}

function Note(props: { children: JSX.Element }) {
  return <p class="card-note">{props.children}</p>;
}

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
  onCreatePr: (folder: string) => void;
}) {
  const target = (): Target | undefined => {
    const tab = props.tab;
    if (tab === undefined) return undefined;
    return {
      folder: tab.launch.spec.cwd,
      account: tab.launch.account?.id ?? null,
    };
  };
  const feed = createPanelFeed(target);
  const [failed, setFailed] = createSignal<string>();
  const act = (doing: Promise<void>) => {
    doing.then(
      () => {
        setFailed(undefined);
      },
      (error: unknown) => setFailed(`Could not open it: ${String(error)}`),
    );
  };
  const changes = () => loadedValue(feed.changes());
  const account = () => props.tab?.launch.account ?? null;
  const accountName = (id: number, fallback: string) =>
    props.profiles.accounts.find((a) => a.id === id)?.name ?? fallback;
  const connectors = () => loadedValue(feed.connectors());
  const skills = () => loadedValue(feed.skills());

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
      <div class="panel-cards">
        <Show
          when={props.tab}
          fallback={
            <p class="panel-empty">Open a tab to see its details here.</p>
          }
        >
          <Card id="environment" title="Environment" state={props.state}>
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
            <Show when={branchText(changes())}>
              {(branch) => <Row icon={<BranchIcon />} text={branch()} />}
            </Show>
          </Card>
          <Card id="changes" title="Changes" state={props.state}>
            <ChangesBody
              folder={props.tab?.launch.spec.cwd ?? null}
              changes={feed.changes()}
              pr={loadedValue(feed.pr()) ?? null}
              onCreatePr={props.onCreatePr}
              onOpenPr={(folder) => {
                act(openPr(folder));
              }}
            />
          </Card>
          <Card
            id="connectors"
            title="Connectors"
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
                        meta={`${server.kind} · ${server.scope}`}
                      />
                    )}
                  </For>
                  <For each={found().unreadable}>
                    {(file) => <Note>Could not read {file}</Note>}
                  </For>
                </>
              )}
            </Show>
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
                    <Row
                      icon={<SkillIcon />}
                      text={skill.name}
                      detail={skill.description}
                      title={skill.description ?? undefined}
                    />
                  )}
                </For>
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

function ChangesBody(props: {
  folder: string | null;
  changes: Loaded<Changes>;
  pr: { number: number; title: string } | null;
  onCreatePr: (folder: string) => void;
  onOpenPr: (folder: string) => void;
}) {
  const repo = () => {
    const found = loadedValue(props.changes);
    return found?.kind === "repo" ? found : undefined;
  };
  return (
    <Switch fallback={<Note>Reading…</Note>}>
      <Match when={props.folder === null}>
        <Note>No project folder</Note>
      </Match>
      <Match when={loadedError(props.changes)}>
        {(error) => <Note>{error()}</Note>}
      </Match>
      <Match when={loadedValue(props.changes)?.kind === "notRepo"}>
        <Note>Not a git repository</Note>
      </Match>
      <Match when={loadedValue(props.changes)?.kind === "noGit"}>
        <Note>Git is not installed</Note>
      </Match>
      <Match when={repo()}>
        {(found) => (
          <>
            <Row
              icon={<DiffIcon />}
              text={found().name}
              title={
                found().base === null
                  ? "Uncommitted work"
                  : `Against ${String(found().base)}, uncommitted work included`
              }
              meta={
                <>
                  <span class="added">+{found().added}</span>
                  <span class="removed">−{found().removed}</span>
                  <button
                    type="button"
                    class="row-button"
                    disabled={!found().gh || found().branch === null}
                    title={
                      found().gh
                        ? "Opens a shell with gh pr create typed, for you to send"
                        : "Install the GitHub CLI (gh) to create a PR from here"
                    }
                    onClick={() => {
                      if (props.folder !== null) props.onCreatePr(props.folder);
                    }}
                  >
                    Create PR
                  </button>
                </>
              }
            />
            <Show when={props.pr}>
              {(pr) => (
                <button
                  type="button"
                  class="card-row action"
                  title="Open in the browser"
                  onClick={() => {
                    if (props.folder !== null) props.onOpenPr(props.folder);
                  }}
                >
                  <span class="row-icon pr">
                    <PrIcon />
                  </span>
                  <span class="row-text">
                    <span class="row-main">{pr().title}</span>
                    <span class="row-detail">
                      {found().name} #{pr().number}
                    </span>
                  </span>
                </button>
              )}
            </Show>
          </>
        )}
      </Match>
    </Switch>
  );
}
