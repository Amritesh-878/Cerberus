import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import {
  activeTitle,
  cleanup,
  emit,
  field,
  open,
  press,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";
import {
  CHAT_MIN,
  initials,
  panelOverlays,
  RAIL_WIDTH,
  sidebarMode,
} from "./sidebarLayout";

const WIDE = 1280;

afterEach(async () => {
  await cleanup();
  setWidth(WIDE);
});

function setWidth(width: number): void {
  Object.defineProperty(window, "innerWidth", {
    value: width,
    configurable: true,
  });
}

const TREE: SavedTree = {
  projects: [
    { id: 1, name: "alpha", folder: String.raw`C:\code\alpha` },
    { id: 2, name: "notes site", folder: String.raw`C:\code\notes` },
  ],
  tabs: [
    { id: 3, projectId: 1, parentId: null, title: "planner" },
    { id: 4, projectId: 2, parentId: null, title: "coder" },
  ].map((tab) => ({
    ...tab,
    program: "claude",
    cwd: String.raw`C:\code\alpha`,
    account: 1,
  })),
  nextId: 5,
};

function sidebarCore(start: Profiles = SAVED) {
  const pty = ptyCore();
  const saves: unknown[] = [];
  let profiles = start;
  return {
    pty,
    saves,
    profiles: () => profiles,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      set_sidebar_collapsed: (args: unknown) => {
        const collapsed = field(args, "collapsed") as boolean;
        saves.push(collapsed);
        profiles = {
          ...profiles,
          sidebarCollapsed: collapsed,
          revision: profiles.revision + 1,
        };
        return profiles;
      },
      set_panel: (args: unknown) => {
        profiles = {
          ...profiles,
          panel: field(args, "panel") as Profiles["panel"],
          revision: profiles.revision + 1,
        };
        return profiles;
      },
    },
  };
}

function railProjects(root: HTMLElement): string[] {
  return [...root.querySelectorAll(".rail-project")].map(
    (b) => b.getAttribute("aria-label") ?? "",
  );
}

function clickLabel(root: HTMLElement, label: string): void {
  const found = root.querySelector<HTMLButtonElement>(
    `[aria-label="${label}"]`,
  );
  if (!found) throw new Error(`no ${label} button`);
  found.click();
}

test("the collapse button folds the sidebar to a rail of projects and saves it", async () => {
  const side = sidebarCore();
  const root = await renderApp(side.profiles, side.core);

  clickLabel(root, "Collapse the sidebar");
  await settle();

  expect(root.querySelector(".app")?.classList.contains("rail")).toBe(true);
  expect(railProjects(root)).toEqual(["alpha", "notes site"]);
  expect(
    [...root.querySelectorAll(".rail-project")].map((b) => b.textContent),
  ).toEqual(["AL", "NS"]);
  expect(root.querySelector(".sidebar-edge")).toBeNull();
  expect(side.saves).toEqual([true]);

  clickLabel(root, "Expand the sidebar");
  await settle();
  expect(root.querySelector(".rail")).toBeNull();
  expect(side.saves).toEqual([true, false]);
});

test("Ctrl+Shift+E collapses and expands the sidebar", async () => {
  const side = sidebarCore();
  const root = await renderApp(side.profiles, side.core);

  press("E", { ctrlKey: true, shiftKey: true });
  await settle();
  expect(root.querySelector(".rail")).not.toBeNull();
  press("E", { ctrlKey: true, shiftKey: true });
  await settle();

  expect(root.querySelector(".rail")).toBeNull();
  expect(side.saves).toEqual([true, false]);
});

test("a saved collapse comes back on start", async () => {
  const side = sidebarCore({ ...SAVED, sidebarCollapsed: true });
  const root = await renderApp(side.profiles, side.core);

  expect(railProjects(root)).toEqual(["alpha", "notes site"]);
  expect(side.saves).toEqual([]);
});

test("the rail marks a project whose chat waits on you, and a click opens that project", async () => {
  const side = sidebarCore();
  const root = await renderApp(side.profiles, side.core);
  await open(root, "planner");
  await open(root, "coder");
  emit(side.pty.spawned, 0, { state: "working" }, { state: "idle" });
  await settle();
  clickLabel(root, "Collapse the sidebar");
  await settle();

  expect(railProjects(root)).toEqual([
    "alpha, a chat is waiting on you",
    "notes site",
  ]);
  expect(
    root.querySelector(".rail-project.active")?.getAttribute("title"),
  ).toBe("notes site");

  clickLabel(root, "alpha, a chat is waiting on you");
  await settle();
  expect(root.querySelector(".rail")).toBeNull();
  expect(side.saves).toEqual([true, false]);
  expect(activeTitle(root)).toBe("coder");
});

test("a narrow window shows the rail without changing the saved choice", async () => {
  setWidth(860);
  const side = sidebarCore();
  const root = await renderApp(side.profiles, side.core);

  expect(root.querySelector(".rail")).not.toBeNull();
  clickLabel(root, "Expand the sidebar");
  await settle();
  expect(root.querySelector(".rail")).toBeNull();
  clickLabel(root, "Collapse the sidebar");
  await settle();
  expect(root.querySelector(".rail")).not.toBeNull();

  setWidth(WIDE);
  window.dispatchEvent(new Event("resize"));
  await settle();
  expect(root.querySelector(".rail")).toBeNull();
  expect(side.saves).toEqual([]);
});

test("the panel goes over the chat when beside it the chat would be too narrow, and closes from its bar", async () => {
  setWidth(1000);
  const side = sidebarCore({
    ...SAVED,
    panel: { ...SAVED.panel, open: true },
  });
  const root = await renderApp(side.profiles, side.core);

  expect(root.querySelector(".app")?.classList.contains("panel-over")).toBe(
    true,
  );
  clickLabel(root, "Close the panel");
  await settle();
  expect(root.querySelector(".panel")).toBeNull();

  setWidth(WIDE);
  window.dispatchEvent(new Event("resize"));
  clickLabel(root, "Right panel");
  await settle();
  expect(root.querySelector(".app")?.classList.contains("panel-over")).toBe(
    false,
  );
  expect(root.querySelector('[aria-label="Close the panel"]')).toBeNull();
});

test("the layout rules", () => {
  expect(sidebarMode(700, false, false)).toBe("drawer");
  expect(sidebarMode(900, false, false)).toBe("rail");
  expect(sidebarMode(900, false, true)).toBe("open");
  expect(sidebarMode(1200, true, false)).toBe("rail");
  expect(sidebarMode(1200, false, false)).toBe("open");
  expect(panelOverlays(1280, 248, 300)).toBe(false);
  expect(panelOverlays(1000, 248, 300)).toBe(true);
  expect(panelOverlays(RAIL_WIDTH + 300 + CHAT_MIN, RAIL_WIDTH, 300)).toBe(
    false,
  );
  expect(initials("alpha")).toBe("AL");
  expect(initials("notes site")).toBe("NS");
  expect(initials("rag_lab")).toBe("RL");
  expect(initials("🦊fox")).toBe("🦊F");
  expect(initials("  ")).toBe("?");
});
