import { afterEach, expect, test } from "vitest";

import { backToShell, startedByHand } from "./handChat";
import {
  CHOICES,
  cleanup,
  click,
  emit,
  field,
  open,
  ptyCore,
  renderApp,
  row,
  SAVED,
  settle,
} from "./harness";
import type { SavedTab, SavedTree } from "./saved";
import { tabFor } from "./tab";
import { addTab, emptyTree } from "./tree";

afterEach(cleanup);

const FOLDER = String.raw`C:\code\alpha`;
const ELSEWHERE = String.raw`C:\code\alpha\server`;

/** One project with a shell tab and a Claude tab, both on "work" (account 1). */
const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: FOLDER }],
  tabs: [
    {
      id: 2,
      projectId: 1,
      parentId: null,
      title: "shell",
      program: "powershell",
      cwd: FOLDER,
      account: 1,
    },
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: FOLDER,
      account: 1,
    },
  ],
  nextId: 4,
};

function core(ended: number[] = []) {
  const pty = ptyCore();
  const saves: SavedTree[] = [];
  const writes: string[] = [];
  const reopened: unknown[] = [];
  return {
    pty,
    saves,
    writes,
    reopened,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null, ended }),
      tree_save: (args: unknown) => {
        saves.push(field(args, "tree") as SavedTree);
      },
      pty_write: (args: unknown) => {
        writes.push(field(args, "data") as string);
      },
      reopen_as_claude: (args: unknown) => {
        reopened.push(field(args, "tab"));
      },
    },
  };
}

function shows(root: HTMLElement, title: string, icon: "claude" | "shell") {
  return row(root, title).querySelector(`.icon-${icon}`) !== null;
}

function savedTab(saves: SavedTree[], id: number): SavedTab | undefined {
  return saves.at(-1)?.tabs.find((t) => t.id === id);
}

async function shiftEnter(root: HTMLElement, app: { writes: string[] }) {
  app.writes.length = 0;
  root.querySelector(".tab-view.shown .xterm-helper-textarea")?.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      code: "Enter",
      keyCode: 13,
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }),
  );
  await settle();
  return app.writes.join("");
}

async function openMenu(root: HTMLElement, title: string): Promise<void> {
  row(root, title).dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, clientX: 20, clientY: 40 }),
  );
  await settle();
}

function menuItems(root: HTMLElement): string[] {
  return [
    ...root.querySelectorAll<HTMLButtonElement>(".tab-menu [role=menuitem]"),
  ].map((b) => b.textContent.trim());
}

test("a claude started in a shell tab makes it a Claude tab while it runs", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "shell");
  expect(await shiftEnter(root, app)).toBe("\r");

  emit(
    app.pty.spawned,
    0,
    { chat: "started", cwd: ELSEWHERE },
    { state: "working" },
  );
  await settle();

  expect(shows(root, "shell", "claude")).toBe(true);
  expect(row(root, "shell").querySelector(".mark.working")).not.toBeNull();
  expect(await shiftEnter(root, app)).toBe("\x1b[13;2u");
  expect(root.querySelector(".hand-bar")?.textContent).toContain(
    "Started by hand: messages need a Claude tab.",
  );
  // Saved as a Claude tab in the chat's folder, so a restart resumes that chat.
  expect(savedTab(app.saves, 2)).toMatchObject({
    program: "claude",
    cwd: ELSEWHERE,
    title: "shell",
  });
  expect(app.pty.spawned).toHaveLength(1);
});

test("when it exits with a chat to resume, the tab is a shell whose ended chat is that one", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "shell");

  emit(
    app.pty.spawned,
    0,
    { chat: "started", cwd: null },
    { state: "idle" },
    { chat: "ended", saved: true },
  );
  await settle();

  expect(shows(root, "shell", "shell")).toBe(true);
  expect(root.querySelector(".hand-bar")).toBeNull();
  expect(await shiftEnter(root, app)).toBe("\r");
  expect(savedTab(app.saves, 2)).toMatchObject({
    program: "claude",
    cwd: FOLDER,
  });
  await openMenu(root, "shell");
  expect(menuItems(root)).toEqual([
    "Resume chat",
    "Continue the ended chat on personal…",
    "Accounts",
  ]);
  expect(app.pty.spawned).toHaveLength(1);
});

test("a chat that never had anything to resume hands the tab back as the shell it was", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "shell");

  emit(
    app.pty.spawned,
    0,
    { chat: "started", cwd: ELSEWHERE },
    { chat: "ended", saved: false },
  );
  await settle();

  expect(shows(root, "shell", "shell")).toBe(true);
  expect(savedTab(app.saves, 2)).toMatchObject({
    program: "powershell",
    cwd: FOLDER,
  });
  await openMenu(root, "shell");
  expect(root.querySelector(".tab-menu")).toBeNull();
});

test("an ended tab running a new claude by hand goes back to its ended chat if that one saves nothing", async () => {
  const app = core([3]);
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "planner");
  expect(shows(root, "planner", "shell")).toBe(true);

  emit(app.pty.spawned, 0, { chat: "started", cwd: null });
  await settle();
  expect(shows(root, "planner", "claude")).toBe(true);
  emit(app.pty.spawned, 0, { chat: "ended", saved: false });
  await settle();

  expect(shows(root, "planner", "shell")).toBe(true);
  await openMenu(root, "planner");
  expect(menuItems(root)[0]).toBe("Resume chat");
});

test("Reopen as Claude tab stops the shell, waits for the core, then starts the chat as a Claude tab", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "shell");
  emit(
    app.pty.spawned,
    0,
    { chat: "started", cwd: ELSEWHERE },
    { state: "idle" },
  );
  await settle();

  click(root.querySelector(".hand-bar") ?? root, "Reopen as Claude tab");
  await settle();

  expect(app.pty.killed).toEqual([1]);
  expect(app.reopened).toEqual([2]);
  expect(app.pty.spawned).toHaveLength(2);
  expect(field(app.pty.spawned[1], "tab")).toBe(2);
  expect(field(field(app.pty.spawned[1], "request"), "program")).toBe(
    CHOICES[0]?.program,
  );
  expect(field(field(app.pty.spawned[1], "request"), "cwd")).toBe(ELSEWHERE);
  expect(root.querySelector(".hand-bar")).toBeNull();
  expect(shows(root, "shell", "claude")).toBe(true);
});

test("a working chat asks before it is stopped to reopen", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "shell");
  emit(
    app.pty.spawned,
    0,
    { chat: "started", cwd: null },
    { state: "working" },
  );
  await settle();
  const bar = () => root.querySelector(".hand-bar") ?? root;

  click(bar(), "Reopen as Claude tab");
  await settle();
  expect(bar().textContent).toContain(
    "Claude is working. Stop it and reopen the chat as a Claude tab?",
  );
  click(bar(), "Cancel");
  await settle();
  expect(app.pty.killed).toEqual([]);
  click(bar(), "Reopen as Claude tab");
  await settle();
  click(bar(), "Stop and reopen");
  await settle();

  expect(app.pty.killed).toEqual([1]);
  expect(app.reopened).toEqual([2]);
});

test("a reopen the core refuses says why and still starts the tab", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, {
    ...app.core,
    reopen_as_claude: () => {
      throw new Error("the chat is still running; close it first");
    },
  });
  await open(root, "shell");
  emit(app.pty.spawned, 0, { chat: "started", cwd: null });
  await settle();

  click(root.querySelector(".hand-bar") ?? root, "Reopen as Claude tab");
  await settle();

  expect(root.querySelector(".tab-alert")?.textContent).toContain(
    "Could not reopen as a Claude tab",
  );
  expect(app.pty.spawned).toHaveLength(2);
});

test("a default title follows the tab's program both ways, and a chosen one stays", () => {
  const shell = CHOICES[1];
  if (!shell) throw new Error("no shell choice");
  const account = { id: 1, name: "work", configDir: String.raw`C:\h\.w` };
  const launch = tabFor(shell, { account, cwd: FOLDER });
  if (!launch) throw new Error("no launch");
  const [tree, id] = addTab(
    emptyTree(),
    { projectId: null, parentId: null },
    launch,
  );

  const claude = startedByHand(tree, id, CHOICES[0], null);
  const back = backToShell(claude, id, launch);

  expect(claude.tabs[0]?.title).toBe("Claude Code · work");
  expect(claude.tabs[0]?.launch.spec.cwd).toBe(FOLDER);
  expect(claude.tabs[0]?.launch.imagePasteKey).toBe("\x1bv");
  expect(back.tabs[0]?.title).toBe("PowerShell · work");
  expect(back.tabs[0]?.launch).toEqual(launch);
});
