import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createEffect, on, onCleanup, onMount, untrack } from "solid-js";

import { type ProjectId, type TabId, tabOrder, type Tree } from "./tree";

/** A tab `cerberus open` asked for; no tab means the project's last shown one. */
export interface OpenTarget {
  projectId: ProjectId;
  tabId: TabId | null;
}

/** The tab named, else the one last shown in that project, else its first; none when it has no tabs. */
export function tabToOpen(
  tree: Tree,
  target: OpenTarget,
  lastShown: ReadonlyMap<ProjectId, TabId>,
): TabId | undefined {
  const inProject = tabOrder(tree).filter(
    (id) => tree.tabs.find((t) => t.id === id)?.projectId === target.projectId,
  );
  const wanted = target.tabId ?? lastShown.get(target.projectId);
  return wanted !== undefined && inProject.includes(wanted)
    ? wanted
    : inProject[0];
}

interface AppView {
  tree: () => Tree;
  shown: () => TabId | undefined;
  restored: () => boolean;
  select: (id: TabId) => void;
  newTabIn: (projectId: ProjectId) => void;
}

/** Shows what `cerberus open` asks for, once the saved tree is in. */
export function followOpenRequests(app: AppView): void {
  const lastShown = new Map<ProjectId, TabId>();
  createEffect(
    on(app.shown, (id) => {
      const projectId =
        id === undefined
          ? null
          : app.tree().tabs.find((t) => t.id === id)?.projectId;
      if (id !== undefined && projectId != null) lastShown.set(projectId, id);
    }),
  );

  const take = () => {
    invoke<OpenTarget | null>("open_take").then(
      (target) => {
        if (!target) return;
        const tree = app.tree();
        const id = tabToOpen(tree, target, lastShown);
        if (id !== undefined) app.select(id);
        else if (tree.projects.some((p) => p.id === target.projectId)) {
          app.newTabIn(target.projectId);
        }
      },
      (error: unknown) => {
        console.error("could not take the open request:", error);
      },
    );
  };

  // A request made before the restore waits in the core until this takes it.
  createEffect(
    on(app.restored, (done) => {
      if (done) take();
    }),
  );
  onMount(() => {
    const listening = listen("open-request", () => {
      if (untrack(app.restored)) take();
    });
    onCleanup(() => {
      void listening.then((stop) => {
        stop();
      });
    });
  });
}
