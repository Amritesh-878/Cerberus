import { afterEach, expect, test } from "vitest";

import {
  activeTitle,
  cleanup,
  click,
  field,
  press,
  ptyCore,
  renderApp,
  SAVED,
  settle,
  sidebarTitles,
} from "./harness";
import type { SavedTab, SavedTree } from "./saved";

afterEach(cleanup);

function saved(
  id: number,
  projectId: number,
  program: string,
  extra: Partial<SavedTab> = {},
): SavedTab {
  return {
    id,
    projectId,
    parentId: null,
    title: `${program} ${String(id)}`,
    program,
    cwd: String.raw`C:\code\p${String(projectId)}`,
    account: 1,
    ...extra,
  };
}

/** Two projects with five tabs: three Claude, two shells, one a sub-tab. */
const TREE: SavedTree = {
  projects: [
    { id: 1, name: "alpha", folder: String.raw`C:\code\p1` },
    { id: 2, name: "beta", folder: String.raw`C:\code\p2` },
  ],
  tabs: [
    saved(3, 1, "claude", { title: "planner" }),
    saved(4, 1, "claude", { title: "coder", parentId: 3, account: 0 }),
    saved(5, 1, "powershell", { title: "shell" }),
    saved(6, 2, "claude", { title: "site" }),
    saved(7, 2, "powershell", { title: "server" }),
  ],
  nextId: 8,
};

function restoring(tree: SavedTree = TREE) {
  const pty = ptyCore();
  const saves: unknown[] = [];
  return {
    pty,
    saves,
    core: {
      ...pty.core,
      tree_load: () => tree,
      tree_save: (args: unknown) => {
        saves.push(field(args, "tree"));
      },
    },
  };
}

function row(root: HTMLElement, title: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".tab-row")].find(
    (r) => r.querySelector(".tab-title")?.textContent === title,
  );
  if (!found) throw new Error(`no ${title} row`);
  return found;
}

async function open(root: HTMLElement, title: string): Promise<void> {
  row(root, title).querySelector<HTMLButtonElement>(".tab-select")?.click();
  await settle();
}

// The mocked core delivers channel messages through the callback the channel registered.
interface MockInternals {
  runCallback: (id: number, data: unknown) => void;
}

const sentCount = new Map<number, number>();

/** Sends `messages` on the channel the `nth` spawn was given, as the core would. */
function emit(spawned: unknown[], nth: number, ...messages: unknown[]): void {
  const channel = field(spawned[nth], "events") as { id: number };
  const internals = (
    window as unknown as { __TAURI_INTERNALS__: MockInternals }
  ).__TAURI_INTERNALS__;
  const sent = (sentCount.get(channel.id) ?? 0) + messages.length;
  messages.forEach((message, i) => {
    internals.runCallback(channel.id, {
      index: sent - messages.length + i,
      message,
    });
  });
  sentCount.set(channel.id, sent);
}
test("the saved tree comes back in order and starts no program", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);

  expect(sidebarTitles(root)).toEqual([
    "planner",
    "  coder",
    "shell",
    "site",
    "server",
  ]);
  expect(
    [...root.querySelectorAll(".project-name span")].map((p) => p.textContent),
  ).toEqual(["alpha", "beta"]);
  expect(restore.pty.spawned).toEqual([]);
  expect(root.querySelectorAll(".tab-view")).toHaveLength(0);
});

test("opening a restored tab starts it in its folder, under its account, as that tab", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);

  await open(root, "coder");

  expect(restore.pty.spawned).toHaveLength(1);
  const spawn = restore.pty.spawned[0];
  expect(field(spawn, "tab")).toBe(4);
  expect(field(spawn, "account")).toBe(0);
  expect(field(spawn, "request")).toMatchObject({
    program: String.raw`C:\bin\claude.exe`,
    cwd: String.raw`C:\code\p1`,
  });
  expect(root.querySelectorAll(".tab-view")).toHaveLength(1);
});

test("a tab started once keeps running when you move to another", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);

  await open(root, "planner");
  await open(root, "site");
  await open(root, "planner");

  expect(restore.pty.spawned.map((s) => field(s, "tab"))).toEqual([3, 6]);
  expect(restore.pty.killed).toEqual([]);
});

test("loading saves nothing; a change saves the tree without setup tabs", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);
  expect(restore.saves).toEqual([]);

  click(row(root, "server"), "Close server");
  await settle();

  expect(restore.saves).toHaveLength(1);
  const tree = restore.saves[0] as SavedTree;
  expect(tree.tabs.map((t) => t.id)).toEqual([3, 4, 5, 6]);
  expect(tree.nextId).toBe(8);
});

test("nothing is saved over a tree that failed to load", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, {
    ...restore.core,
    tree_load: () => {
      throw new Error("saved tabs are unavailable: fake");
    },
  });

  expect(root.textContent).toContain("saved tabs are unavailable: fake");
  click(root, "New project");
  await settle();
  expect(restore.saves).toEqual([]);
});

test("a turn that ends in another tab waits on you until you open it", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);
  await open(root, "planner");
  await open(root, "site");

  emit(restore.pty.spawned, 0, { state: "working" });
  await settle();
  expect(row(root, "planner").querySelector(".mark.working")).not.toBeNull();

  emit(restore.pty.spawned, 0, { state: "idle" });
  await settle();
  expect(row(root, "planner").querySelector(".mark.waiting")).not.toBeNull();

  press("A", { ctrlKey: true, shiftKey: true });
  await settle();
  expect(activeTitle(root)).toBe("planner");
  expect(row(root, "planner").querySelector(".mark")).toBeNull();
});

test("a turn that ends on the tab you are on shows no mark", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);
  await open(root, "planner");

  emit(restore.pty.spawned, 0, { state: "working" }, { state: "idle" });
  await settle();

  expect(row(root, "planner").querySelector(".mark")).toBeNull();
});

test("a limit shows a clock with its reset time, and an exit dims the row", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, restore.core);
  await open(root, "site");
  const resetsAt = new Date(2026, 9, 29, 17, 50).getTime() / 1000;

  emit(restore.pty.spawned, 0, { state: "limited", resetsAt });
  await settle();
  const mark = row(root, "site").querySelector(".mark.limited");
  expect(mark?.textContent).toMatch(/5:50/);
  expect(mark?.getAttribute("title")).toMatch(/^Usage limit, resets 5:50/);

  emit(restore.pty.spawned, 0, { exitCode: 0 });
  await settle();
  expect(row(root, "site").classList.contains("done")).toBe(true);
  expect(row(root, "site").querySelector(".mark")).toBeNull();
});

test("a tab whose saved chat is gone says it started a new one", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, {
    ...restore.core,
    pty_spawn: () => ({ id: 1, resume: "missing" }),
  });

  await open(root, "planner");

  expect(root.querySelector(".terminal-notice")?.textContent).toBe(
    "The saved chat is gone, so this is a new one.",
  );
});

test("a resuming tab says so until the chat reports in", async () => {
  const restore = restoring();
  const root = await renderApp(() => SAVED, {
    ...restore.core,
    pty_spawn: (args: unknown) => {
      restore.pty.spawned.push(args);
      return { id: 1, resume: "resuming" };
    },
  });
  await open(root, "planner");
  expect(root.querySelector(".terminal-notice")?.textContent).toBe(
    "Resuming chat…",
  );

  emit(restore.pty.spawned, 0, { state: "idle" });
  await settle();

  expect(root.querySelector(".terminal-notice")).toBeNull();
});
