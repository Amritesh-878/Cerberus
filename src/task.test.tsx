import { afterEach, expect, test } from "vitest";

import {
  cleanup,
  click,
  emit,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
  sidebarTitles,
} from "./harness";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const CODE = String.raw`C:\p\code`;
const TASKS = String.raw`C:\p\plans\tasks`;
const WORKTREE = String.raw`C:\p\code-task-025`;
const DEFAULT = "Read AGENTS.md, then {task_file}.";

const TREE: SavedTree = {
  projects: [{ id: 1, name: "code", folder: CODE }],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: CODE,
      account: 0,
      dirs: [],
    },
  ],
  nextId: 4,
};

const FILES = [
  {
    path: String.raw`C:\p\plans\tasks\task-025.md`,
    id: "TASK-025",
    heading: "TASK-025: Start a task",
    modifiedMs: 2000,
  },
  {
    path: String.raw`C:\p\plans\tasks\task-024.md`,
    id: "TASK-024",
    heading: "TASK-024: File pane",
    modifiedMs: 1000,
  },
];

function taskCore(options: { worktreeFails?: string } = {}) {
  const pty = ptyCore();
  const writes: string[] = [];
  const saves: SavedTree[] = [];
  const calls: { cmd: string; args: unknown }[] = [];
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  return {
    pty,
    writes,
    saves,
    calls,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null, ended: [] }),
      tree_save: (args: unknown) => {
        saves.push(field(args, "tree") as SavedTree);
      },
      pty_write: (args: unknown) => {
        writes.push(String(field(args, "data")));
      },
      panel_changes: () => ({ kind: "notRepo" }),
      panel_connectors: () => ({ servers: [], unreadable: [] }),
      panel_skills: () => [],
      ...record("task_folder", () => TASKS),
      ...record("task_files", () => FILES),
      task_default_prompt: () => DEFAULT,
      task_worktree_defaults: () => ({
        folder: WORKTREE,
        branch: "feat/task-025",
      }),
      ...record("task_branches", () => ({
        default: "main",
        names: ["dev", "main"],
      })),
      ...record("task_worktree", () => {
        if (options.worktreeFails) throw new Error(options.worktreeFails);
        return null;
      }),
      ...record("task_plan", (args) => {
        const request = field(args, "request");
        const worktree = field(request, "worktree") as [string, string] | null;
        const template = field(request, "template") as string | null;
        return {
          title: "TASK-025 · Start a task",
          prompt: `${template ?? DEFAULT} [${worktree?.[1] ?? "main"}]`,
          cwd: worktree?.[0] ?? CODE,
          dirs: worktree ? [CODE] : [],
        };
      }),
    },
  };
}

function dialog(): HTMLElement {
  const found = document.querySelector<HTMLElement>(".task-dialog");
  if (!found) throw new Error("no task dialog");
  return found;
}

async function startTask(root: HTMLElement): Promise<void> {
  await open(root, "planner");
  root.querySelector<HTMLButtonElement>(".account-pill")?.click();
  await settle();
  click(root.querySelector(".tab-menu") ?? root, "Start a task…");
  await settle();
  await settle();
}

function type(el: HTMLInputElement | HTMLTextAreaElement, text: string): void {
  el.value = text;
  el.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

const project = (saves: SavedTree[]) => saves.at(-1)?.projects[0];

test("the dialog lists the guessed task folder's files newest first, the newest picked, on the tab's account", async () => {
  const env = taskCore();
  const root = await renderApp(() => SAVED, env.core);

  await startTask(root);

  const asked = env.calls.find((c) => c.cmd === "task_folder");
  expect(field(asked?.args, "folders")).toEqual([CODE, CODE]);
  const rows = [...dialog().querySelectorAll(".task-file")].map((r) =>
    r.textContent.trim(),
  );
  expect(rows).toEqual([
    "TASK-025TASK-025: Start a task",
    "TASK-024TASK-024: File pane",
  ]);
  const checked = dialog().querySelector<HTMLInputElement>(
    ".task-file input:checked",
  );
  expect(checked?.closest(".task-file")?.textContent).toContain("TASK-025");
  const pressed = dialog().querySelector(
    '.choices button[aria-pressed="true"]',
  );
  expect(pressed?.textContent).toBe("personal");
});

test("Open adds a sub-tab named for the task and types its prompt only once the chat is idle, never with Enter", async () => {
  const env = taskCore();
  const root = await renderApp(() => SAVED, env.core);
  await startTask(root);

  click(dialog(), "Open");
  await settle();
  await settle();

  expect(document.querySelector(".task-dialog")).toBeNull();
  expect(sidebarTitles(root)).toEqual(["planner", "  TASK-025 · Start a task"]);
  const coder = env.pty.spawned.at(-1);
  expect(field(field(coder, "request"), "cwd")).toBe(CODE);
  expect(field(coder, "account")).toBe(0);
  const index = env.pty.spawned.length - 1;

  emit(env.pty.spawned, index, { state: "working" });
  await settle();
  expect(env.writes).toEqual([]);

  emit(env.pty.spawned, index, { state: "idle" });
  await settle();
  emit(env.pty.spawned, index, { state: "idle" });
  await settle();
  expect(env.writes).toEqual([`\x1b[200~${DEFAULT} [main]\x1b[201~`]);
  expect(env.writes.join("")).not.toContain("\r");
  expect(project(env.saves)?.tasks).toEqual({
    folder: null,
    prompt: null,
    account: 0,
  });
});

test("a new worktree is made first, from the default branch, and the chat runs there with the project folder added", async () => {
  const env = taskCore();
  const root = await renderApp(() => SAVED, env.core);
  await startTask(root);

  click(dialog(), "New worktree");
  await settle();
  await settle();
  const select = dialog().querySelector("select");
  expect(select?.value).toBe("main");
  const inputs = [
    ...dialog().querySelectorAll<HTMLInputElement>(".env-field input"),
  ];
  expect(inputs.map((i) => i.value)).toEqual([WORKTREE, "feat/task-025"]);
  click(dialog(), "Open");
  await settle();
  await settle();

  const made = env.calls.find((c) => c.cmd === "task_worktree");
  expect([
    field(made?.args, "repo"),
    field(made?.args, "folder"),
    field(made?.args, "branch"),
    field(made?.args, "base"),
  ]).toEqual([CODE, WORKTREE, "feat/task-025", "main"]);
  const request = field(env.pty.spawned.at(-1), "request");
  expect(field(request, "cwd")).toBe(WORKTREE);
  expect(field(request, "dirs")).toEqual([CODE]);
});

test("a worktree git refuses opens no tab and shows git's message", async () => {
  const env = taskCore({
    worktreeFails: "fatal: a branch named 'feat/task-025' already exists",
  });
  const root = await renderApp(() => SAVED, env.core);
  await startTask(root);
  const spawnedBefore = env.pty.spawned.length;

  click(dialog(), "New worktree");
  await settle();
  await settle();
  click(dialog(), "Open");
  await settle();
  await settle();

  expect(dialog().querySelector('[role="alert"]')?.textContent).toContain(
    "a branch named 'feat/task-025' already exists",
  );
  expect(sidebarTitles(root)).toEqual(["planner"]);
  expect(env.pty.spawned).toHaveLength(spawnedBefore);
  expect(env.calls.map((c) => c.cmd)).not.toContain("task_plan");
});

test("a two-line task prompt arrives as one paste, with no line end outside it to send it", async () => {
  const env = taskCore();
  const root = await renderApp(() => SAVED, env.core);
  await startTask(root);
  const area = dialog().querySelector("textarea");
  if (!area) throw new Error("no prompt editor");
  type(area, "Read {task_file}.\r\nThen do it.");

  click(dialog(), "Open");
  await settle();
  await settle();
  emit(env.pty.spawned, env.pty.spawned.length - 1, { state: "idle" });
  await settle();

  expect(env.writes).toHaveLength(1);
  const typed = env.writes[0] ?? "";
  expect(typed.startsWith("\x1b[200~")).toBe(true);
  expect(typed.endsWith("\x1b[201~")).toBe(true);
  const inside = typed.slice("\x1b[200~".length, -"\x1b[201~".length);
  expect(inside).toBe("Read {task_file}.\nThen do it. [main]");
  expect(typed).not.toContain("\r");
});

test("an edited task prompt is used and remembered for the project, and Reset goes back to the default", async () => {
  const env = taskCore();
  const root = await renderApp(() => SAVED, env.core);
  await startTask(root);

  const area = dialog().querySelector("textarea");
  if (!area) throw new Error("no prompt editor");
  expect(area.value).toBe(DEFAULT);
  type(area, "Do {task_id} now.");
  click(dialog(), "Open");
  await settle();
  await settle();

  const plan = env.calls.find((c) => c.cmd === "task_plan");
  expect(field(field(plan?.args, "request"), "template")).toBe(
    "Do {task_id} now.",
  );
  expect(project(env.saves)?.tasks?.prompt).toBe("Do {task_id} now.");

  await startTask(root);
  const again = dialog().querySelector("textarea");
  expect(again?.value).toBe("Do {task_id} now.");
  click(dialog(), "Reset to default");
  await settle();
  expect(dialog().querySelector("textarea")?.value).toBe(DEFAULT);
});

test("the planner's panel opens the same dialog", async () => {
  const env = taskCore();
  const root = await renderApp(
    () => ({ ...SAVED, panel: { ...SAVED.panel, open: true } }),
    env.core,
  );
  await open(root, "planner");

  click(root.querySelector(".panel") ?? root, "Start a task…");
  await settle();
  await settle();

  expect(dialog().querySelector("h2")?.textContent).toBe("Start a task");
});
