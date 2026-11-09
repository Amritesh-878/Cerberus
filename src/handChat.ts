import { batch, createSignal } from "solid-js";

import type { HandChat, ProgramChoice } from "./pty";
import { reopenAsClaude } from "./saved";
import type { Tab } from "./tab";
import { defaultTitle, type TabId, type Tree, type TreeTab } from "./tree";

/** What a shell tab was before a `claude` started in it, so a chat that never saved can hand the tab back. */
export interface HandStart {
  /** The tab's run when it started, so a restart of the tab forgets it. */
  run: number;
  launch: Tab;
  ended: boolean;
}

function withLaunch(
  tree: Tree,
  id: TabId,
  launch: (tab: TreeTab) => Tab,
): Tree {
  return {
    ...tree,
    tabs: tree.tabs.map((t) => {
      if (t.id !== id) return t;
      const next = launch(t);
      const title =
        t.title === defaultTitle(t.launch) ? defaultTitle(next) : t.title;
      return { ...t, title, launch: next };
    }),
  };
}

/** The tab as a Claude tab on its own account, in the chat's folder, so a restart resumes that chat. */
export function startedByHand(
  tree: Tree,
  id: TabId,
  claude: ProgramChoice | undefined,
  cwd: string | null,
): Tree {
  return withLaunch(tree, id, (t) => ({
    ...t.launch,
    program: "claude",
    label: claude?.label ?? "Claude Code",
    spec: {
      ...t.launch.spec,
      program: claude?.program ?? "claude",
      args: claude?.args ?? [],
      cwd: cwd ?? t.launch.spec.cwd,
    },
    imagePasteKey: claude?.imagePasteKey ?? null,
    input: null,
  }));
}

/** The tab as it was before the chat, which ended without anything to resume. */
export function backToShell(tree: Tree, id: TabId, before: Tab): Tree {
  return withLaunch(tree, id, () => before);
}

/** What the hand-started chats need from the app. */
export interface HandApp {
  tree: () => Tree;
  setTree: (tree: Tree) => void;
  claude: () => ProgramChoice | undefined;
  runOf: (id: TabId) => number;
  setRun: (id: TabId, run: number) => void;
  isEnded: (id: TabId) => boolean;
  markEnded: (id: TabId, on: boolean) => void;
  /** Forgets the tab's board mark. */
  clear: (id: TabId) => void;
  /** The tab's chat ended by itself, as when a Claude tab's `claude` exits. */
  ended: (id: TabId) => void;
  setAlert: (id: TabId, alert: string | undefined) => void;
}

/** Shell tabs running a `claude` the user started, which the tab follows as its chat until it ends. */
export function createHandChats(app: HandApp) {
  const [starts, setStarts] = createSignal<ReadonlyMap<TabId, HandStart>>(
    new Map(),
  );
  const forget = (id: TabId) => {
    setStarts((all) => {
      if (!all.has(id)) return all;
      const next = new Map(all);
      next.delete(id);
      return next;
    });
  };
  const startOf = (id: TabId) => {
    const start = starts().get(id);
    return start?.run === app.runOf(id) ? start : undefined;
  };

  const heard = (id: TabId, chat: HandChat) => {
    const tab = app.tree().tabs.find((t) => t.id === id);
    if (!tab) return;
    if (chat.chat === "started") {
      batch(() => {
        const start = {
          run: app.runOf(id),
          launch: tab.launch,
          ended: app.isEnded(id),
        };
        setStarts((all) => new Map(all).set(id, start));
        app.setTree(startedByHand(app.tree(), id, app.claude(), chat.cwd));
        app.markEnded(id, false);
        app.clear(id);
      });
      return;
    }
    const start = startOf(id);
    batch(() => {
      forget(id);
      if (chat.saved) {
        app.ended(id);
        return;
      }
      // Nothing to resume, so the tab is what it was before the chat.
      if (start) {
        app.setTree(backToShell(app.tree(), id, start.launch));
        app.markEnded(id, start.ended);
      }
      app.clear(id);
    });
  };

  /** Stops the shell and its `claude`, then starts the tab as a Claude tab resuming that chat. */
  const reopen = (id: TabId) => {
    const run = app.runOf(id);
    batch(() => {
      forget(id);
      app.setRun(id, 0);
      app.clear(id);
      app.setAlert(id, undefined);
    });
    reopenAsClaude(id).then(
      () => {
        batch(() => {
          app.markEnded(id, false);
          app.setRun(id, run + 1);
        });
      },
      (error: unknown) => {
        batch(() => {
          app.setAlert(
            id,
            `Could not reopen as a Claude tab: ${String(error)}`,
          );
          app.setRun(id, run + 1);
        });
      },
    );
  };

  return {
    byHand: (id: TabId) => startOf(id) !== undefined,
    heard,
    reopen,
    forget,
  };
}
