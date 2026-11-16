import { expect, test, vi } from "vitest";

import { CHOICES, SAVED } from "./harness";
import type { ProgramChoice } from "./pty";
import {
  addCoderTab,
  plannerFor,
  Prefills,
  taskAccount,
  withProjectTasks,
} from "./startTask";
import { tabFor } from "./tab";
import type { Tree, TreeTab } from "./tree";

function choice(at: number): ProgramChoice {
  const found = CHOICES[at];
  if (!found) throw new Error("harness choices changed");
  return found;
}

const CLAUDE = choice(0);
const SHELL = choice(1);

function tab(id: number, parentId: number | null, program = CLAUDE): TreeTab {
  const launch = tabFor(program, {
    account: SAVED.accounts[0] ?? null,
    cwd: String.raw`C:\code`,
  });
  if (!launch) throw new Error("no launch");
  return { id, projectId: 1, parentId, title: `tab ${String(id)}`, launch };
}

const TREE: Tree = {
  projects: [{ id: 1, name: "code", folder: String.raw`C:\code` }],
  tabs: [tab(2, null), tab(3, 2), tab(4, null, SHELL)],
  nextId: 5,
};

const PLAN = {
  title: "TASK-025 · Start a task",
  prompt: "Read AGENTS.md, then the task.",
  cwd: String.raw`C:\code-task-025`,
  dirs: [String.raw`C:\code`],
  taskId: "TASK-025",
  handoffFile: String.raw`C:\plans\completed\task-025.md`,
};

test("a task's coder goes under the planner, whether started from it or from a coder under it", () => {
  const [planner, coder, shell] = TREE.tabs;
  if (!planner || !coder || !shell) throw new Error("tree changed");

  expect(plannerFor(TREE, planner)?.id).toBe(2);
  expect(plannerFor(TREE, coder)?.id).toBe(2);
  expect(plannerFor(TREE, shell)).toBeUndefined();
});

test("the coder tab is a sub-tab named for the task, in the plan's folder with the project folder added", () => {
  const planner = TREE.tabs[0];
  if (!planner) throw new Error("tree changed");

  const added = addCoderTab(
    TREE,
    planner,
    CLAUDE,
    SAVED.accounts[1] ?? null,
    PLAN,
    1_800_000_000_000,
  );

  if (!added) throw new Error("no tab added");
  const [tree, id] = added;
  const coder = tree.tabs.find((t) => t.id === id);
  expect(id).toBe(5);
  expect(coder?.parentId).toBe(2);
  expect(coder?.projectId).toBe(1);
  expect(coder?.title).toBe("TASK-025 · Start a task");
  expect(coder?.launch.program).toBe("claude");
  expect(coder?.launch.spec.cwd).toBe(String.raw`C:\code-task-025`);
  expect(coder?.launch.spec.dirs).toEqual([String.raw`C:\code`]);
  expect(coder?.launch.spec.account).toBe(1);
  expect(coder?.launch.input).toBeNull();
  expect(coder?.handoff).toEqual({
    taskId: "TASK-025",
    file: String.raw`C:\plans\completed\task-025.md`,
    since: 1_800_000_000_000,
    state: "working",
  });
});

test("the account is the project's last, while it exists, then the tab's own, then the default", () => {
  const planner = TREE.tabs[0];
  if (!planner) throw new Error("tree changed");
  const tasks = (account: number | null) => ({
    folder: null,
    prompt: null,
    account,
  });

  expect(taskAccount(tasks(1), planner, SAVED)).toBe(1);
  expect(taskAccount(tasks(9), planner, SAVED)).toBe(0);
  expect(taskAccount(undefined, planner, SAVED)).toBe(0);
  const noAccount = {
    ...planner,
    launch: { ...planner.launch, account: null },
  };
  expect(taskAccount(undefined, noAccount, SAVED)).toBe(1);
});

test("a project's task settings change field by field and stay with that project", () => {
  const once = withProjectTasks(TREE, 1, {
    folder: String.raw`C:\plans\tasks`,
  });
  const twice = withProjectTasks(once, 1, { account: 0 });

  expect(twice.projects[0]?.tasks).toEqual({
    folder: String.raw`C:\plans\tasks`,
    prompt: null,
    account: 0,
  });
  expect(withProjectTasks(TREE, 7, { account: 0 })).toEqual(TREE);
});

test("the prompt is typed once, on the chat's first idle report, and never before", () => {
  const prefills = new Prefills();
  const type = vi.fn();
  prefills.set(5, "Read the task.");

  prefills.reported(5, { state: "working" }, type);
  prefills.reported(6, { state: "idle" }, type);
  prefills.reported(5, { state: "idle" }, undefined);
  expect(type).not.toHaveBeenCalled();

  prefills.reported(5, { state: "idle" }, type);
  prefills.reported(5, { state: "idle" }, type);
  expect(type).toHaveBeenCalledTimes(1);
  expect(type).toHaveBeenCalledWith("Read the task.");
});

test("a closed tab's waiting prompt is dropped", () => {
  const prefills = new Prefills();
  const type = vi.fn();
  prefills.set(5, "Read the task.");

  prefills.forget(5);
  prefills.reported(5, { state: "idle" }, type);

  expect(type).not.toHaveBeenCalled();
});
