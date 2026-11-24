import { createSignal, For, onMount, Show } from "solid-js";

import { Note } from "./Cards";
import { BranchIcon, MoreIcon } from "./icons";
import {
  panelWorktrees,
  type Refusal,
  type Removed,
  removeWorktrees,
  type Worktree,
  type WorktreeView,
} from "./panelData";
import { type MenuItem, RowMenu } from "./RowMenu";

export type TabKind = "shell" | "claude";

export interface WorktreeActions {
  openVsCode: (path: string) => void;
  openFolder: (path: string) => void;
  /** Opens a new tab there beside the shown one; absent where the shown tab has no project. */
  openTab?: (path: string, kind: TabKind) => void;
  /** The project's "Remove merged worktrees automatically"; absent where the shown tab has no project. */
  cleanMerged?: boolean;
  /** Saves that setting; turning it on also removes the merged ones now. */
  setCleanMerged?: (on: boolean) => Promise<void>;
}

const PR_TEXT: Record<NonNullable<Worktree["pr"]>["state"], string> = {
  draft: "draft",
  open: "open",
  merged: "merged",
  closed: "closed",
};

/** Why a worktree stays, in words; `tabs` names the tabs running in it. */
export function refusalText(
  refusal: Refusal,
  tabs: readonly string[] = [],
): string {
  switch (refusal) {
    case "main":
      return "it is the repo's main folder";
    case "locked":
      return "git has it locked";
    case "dirty":
      return "it has uncommitted changes";
    case "unread":
      return "git could not tell whether it is clean";
    case "inUse":
      return tabs.length > 0
        ? `a tab runs in it (${tabs.join(", ")})`
        : "a tab runs in it";
    case "unknown":
      return "it is not a worktree of this repo";
    case "failed":
      return "git could not remove it";
  }
}

/** The row's second line: branch, PR, and anything that keeps it. */
export function worktreeDetail(
  worktree: Worktree,
  tabs: readonly string[],
): string {
  const parts = [
    worktree.branch ?? `detached at ${worktree.commit ?? "HEAD"}`,
    worktree.pr === null
      ? "no PR"
      : `#${String(worktree.pr.number)} ${PR_TEXT[worktree.pr.state]}`,
    worktree.gone ? "remote branch gone" : null,
    worktree.missing ? "folder gone" : null,
    worktree.dirty ? "uncommitted changes" : null,
    worktree.locked ? "locked" : null,
    tabs.length > 0 ? `in ${tabs.join(", ")}` : null,
  ];
  return parts.filter((part) => part !== null).join(" · ");
}

export function removedText(removed: Removed): string {
  if (removed.refusal !== null) {
    return `${removed.name}: kept, ${refusalText(removed.refusal)}`;
  }
  if (removed.branch === null) return `${removed.name}: removed`;
  return removed.branchDeleted
    ? `${removed.name}: removed, merged branch ${removed.branch} deleted here`
    : `${removed.name}: removed, branch ${removed.branch} kept`;
}

/** A repo's worktrees, read when shown; Remove asks first, and the core decides what may go. */
export function WorktreeList(props: {
  tab: number;
  folder: string;
  vscode: boolean;
  tabTitle: (id: number) => string | undefined;
  actions: WorktreeActions;
}) {
  const [view, setView] = createSignal<WorktreeView>();
  const rows = () => view()?.rows;
  const [error, setError] = createSignal<string>();
  const [menuFor, setMenuFor] = createSignal<string>();
  // The list the user is asked about, or the one worktree that cannot go.
  const [asking, setAsking] = createSignal<Worktree[]>();
  const [results, setResults] = createSignal<Removed[]>();
  const [busy, setBusy] = createSignal(false);
  const load = () =>
    panelWorktrees(props.tab, props.folder).then(
      (found) => {
        setView(found);
        setError(undefined);
      },
      (failed: unknown) => setError(String(failed)),
    );
  onMount(() => void load());
  const names = (worktree: Worktree) =>
    worktree.tabs.map((id) => props.tabTitle(id) ?? `tab ${String(id)}`);
  const stale = () => (rows() ?? []).filter((w) => w.stale);
  // Keyed by path, so a reload keeps each row and an open menu in place.
  const paths = () => (rows() ?? []).map((w) => w.path);
  const ask = (list: Worktree[]) => {
    setResults(undefined);
    setAsking(list);
  };
  const confirm = (list: Worktree[]) => {
    setBusy(true);
    removeWorktrees(
      props.tab,
      props.folder,
      list.map((w) => w.path),
    )
      .then(
        (done) => {
          setResults(done);
          setAsking(undefined);
          return load();
        },
        (failed: unknown) => setError(String(failed)),
      )
      .finally(() => setBusy(false));
  };
  const menuItems = (worktree: Worktree): MenuItem[] => {
    const items: MenuItem[] = [];
    if (!worktree.missing) {
      if (props.vscode) {
        items.push({
          label: "Open in VS Code",
          pick: () => {
            props.actions.openVsCode(worktree.path);
          },
        });
      }
      items.push({
        label: "Show in file manager",
        pick: () => {
          props.actions.openFolder(worktree.path);
        },
      });
      const openTab = props.actions.openTab;
      if (openTab) {
        items.push(
          {
            label: "New shell tab here",
            pick: () => {
              openTab(worktree.path, "shell");
            },
          },
          {
            label: "New Claude tab here",
            pick: () => {
              openTab(worktree.path, "claude");
            },
          },
        );
      }
    }
    if (!worktree.main) {
      items.push({
        label: "Remove…",
        pick: () => {
          ask([worktree]);
        },
      });
    }
    return items;
  };
  return (
    <div class="worktrees">
      <div class="worktrees-head">
        <span>Worktrees</span>
        <Show when={rows()}>
          {(found) => <span class="faint">{found().length}</span>}
        </Show>
        <Show when={stale().length > 0 && asking() === undefined}>
          <button
            type="button"
            class="row-button"
            onClick={() => {
              ask(stale());
            }}
          >
            Remove stale ({stale().length})
          </button>
        </Show>
      </div>
      <Show
        when={
          view()?.projectRepo === true &&
          props.actions.cleanMerged !== undefined
        }
      >
        <label class="worktrees-auto">
          <input
            type="checkbox"
            checked={props.actions.cleanMerged === true}
            disabled={busy()}
            onChange={(event) => {
              const save = props.actions.setCleanMerged;
              if (!save) return;
              setBusy(true);
              save(event.currentTarget.checked)
                .then(load, (failed: unknown) => setError(String(failed)))
                .finally(() => setBusy(false));
            }}
          />
          Remove merged worktrees automatically
        </label>
      </Show>
      <Show when={error()}>{(message) => <Note>{message()}</Note>}</Show>
      <For each={view()?.cleaned ?? []}>{(line) => <Note>{line}</Note>}</For>
      <Show when={asking()}>
        {(list) => (
          <div class="worktree-confirm" role="alertdialog">
            <Show
              when={
                list().length === 1 && list()[0]?.refusal != null
                  ? list()[0]
                  : undefined
              }
              fallback={
                <>
                  <p>
                    Remove {list().length === 1 ? "this worktree" : "these"}? A
                    branch is deleted only if merged. GitHub is untouched.
                  </p>
                  <ul>
                    <For each={list()}>
                      {(w) => (
                        <li title={w.path}>
                          {w.name}
                          <span class="faint"> {w.branch ?? "detached"}</span>
                        </li>
                      )}
                    </For>
                  </ul>
                  <div class="worktree-confirm-buttons">
                    <button
                      type="button"
                      class="row-button"
                      disabled={busy()}
                      onClick={() => {
                        confirm(list());
                      }}
                    >
                      Remove
                    </button>
                    <button
                      type="button"
                      class="row-button"
                      disabled={busy()}
                      onClick={() => {
                        setAsking(undefined);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </>
              }
            >
              {(refused) => (
                <>
                  <p>
                    {refused().name} can't be removed:{" "}
                    {refusalText(
                      refused().refusal ?? "unknown",
                      names(refused()),
                    )}
                    .
                  </p>
                  <div class="worktree-confirm-buttons">
                    <button
                      type="button"
                      class="row-button"
                      onClick={() => {
                        setAsking(undefined);
                      }}
                    >
                      Close
                    </button>
                  </div>
                </>
              )}
            </Show>
          </div>
        )}
      </Show>
      <Show when={results()}>
        {(done) => (
          <For each={done()}>
            {(removed) => <Note>{removedText(removed)}</Note>}
          </For>
        )}
      </Show>
      <For each={paths()}>
        {(path) => (
          <Show when={rows()?.find((w) => w.path === path)}>
            {(worktree) => (
              <div
                class="card-row worktree-row"
                classList={{ stale: worktree().stale }}
              >
                <span class="row-icon">
                  <BranchIcon />
                </span>
                <span class="row-text">
                  <span class="row-main" title={worktree().path}>
                    {worktree().name}
                    <Show when={worktree().main}>
                      <span class="worktree-tag">main</span>
                    </Show>
                    <Show when={worktree().stale}>
                      <span class="worktree-tag stale">stale</span>
                    </Show>
                  </span>
                  <span class="row-detail">
                    {worktreeDetail(worktree(), names(worktree()))}
                  </span>
                </span>
                <button
                  type="button"
                  class="row-button icon"
                  aria-label={`More for ${worktree().name}`}
                  aria-haspopup="menu"
                  aria-expanded={menuFor() === path}
                  onClick={() =>
                    setMenuFor((open) => (open === path ? undefined : path))
                  }
                >
                  <MoreIcon />
                </button>
                <Show when={menuFor() === path}>
                  <RowMenu
                    items={menuItems(worktree())}
                    onClose={() => {
                      setMenuFor(undefined);
                    }}
                  />
                </Show>
              </div>
            )}
          </Show>
        )}
      </For>
    </div>
  );
}
