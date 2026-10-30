import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";

import {
  activeTitle,
  cleanup,
  ptyCore,
  renderApp,
  SAVED,
  settle,
  sidebarTitles,
} from "./harness";
import { type OpenTarget, tabToOpen } from "./open";
import { fromSaved, type SavedTab, type SavedTree } from "./saved";

afterEach(cleanup);

function saved(
  id: number,
  projectId: number,
  title: string,
  parentId: number | null = null,
): SavedTab {
  return {
    id,
    projectId,
    parentId,
    title,
    program: "claude",
    cwd: null,
    account: 1,
  };
}

const TREE: SavedTree = {
  projects: [
    { id: 1, name: "alpha", folder: String.raw`C:\code\alpha` },
    { id: 2, name: "beta", folder: String.raw`C:\code\beta` },
    { id: 8, name: "empty", folder: String.raw`C:\code\empty` },
  ],
  tabs: [
    saved(5, 1, "shell"),
    saved(3, 1, "planner"),
    saved(4, 1, "coder", 3),
    saved(6, 2, "site"),
  ],
  nextId: 9,
};

const tree = () => fromSaved(TREE, SAVED, []);

function target(projectId: number, tabId: number | null): OpenTarget {
  return { projectId, tabId };
}

test("a named tab is the one opened", () => {
  expect(tabToOpen(tree(), target(1, 4), new Map())).toBe(4);
});

test("a project alone opens its last shown tab, else its first", () => {
  expect(tabToOpen(tree(), target(1, null), new Map([[1, 3]]))).toBe(3);
  expect(tabToOpen(tree(), target(1, null), new Map([[2, 6]]))).toBe(5);
});

test("a tab that is gone or in another project falls back to the first", () => {
  expect(tabToOpen(tree(), target(1, 99), new Map())).toBe(5);
  expect(tabToOpen(tree(), target(1, 6), new Map())).toBe(5);
});

test("a project with no tabs has nothing to open", () => {
  expect(tabToOpen(tree(), target(8, null), new Map())).toBeUndefined();
});

/** A core that hands out each queued request once, as `open_take` does. */
function opening(...queued: OpenTarget[]) {
  return {
    queued,
    core: {
      ...ptyCore().core,
      tree_load: () => ({ tree: TREE, notice: null }),
      open_take: () => queued.shift() ?? null,
    },
  };
}

test("a request made before the window was ready is shown once the tabs are in", async () => {
  const requests = opening(target(1, 4));

  const root = await renderApp(() => SAVED, requests.core);
  await settle();

  expect(sidebarTitles(root)).toContain("  coder");
  expect(activeTitle(root)).toBe("coder");
});

test("a later request switches tabs and a project alone returns to its last tab", async () => {
  const requests = opening(target(1, 4));
  const root = await renderApp(() => SAVED, requests.core);
  await settle();

  requests.queued.push(target(2, 6));
  await emitEvent("open-request");
  await settle();
  expect(activeTitle(root)).toBe("site");

  requests.queued.push(target(1, null));
  await emitEvent("open-request");
  await settle();
  expect(activeTitle(root)).toBe("coder");
});

test("a project with no tabs opens the new tab picker for it", async () => {
  const requests = opening(target(8, null));

  const root = await renderApp(() => SAVED, requests.core);
  await settle();

  expect(root.querySelector(".new-tab")?.textContent).toContain("empty");
});
