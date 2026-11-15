import { invoke } from "@tauri-apps/api/core";

import type { Account, AccountId, Profiles } from "./accounts";
import type { LiveState } from "./board";
import type { ProgramChoice } from "./pty";
import { tabFor } from "./tab";
import {
  addTab,
  isPlanner,
  type ProjectId,
  type ProjectTasks,
  renameTab,
  type TabId,
  type Tree,
  type TreeTab,
} from "./tree";

export interface TaskFile {
  path: string;
  /** `TASK-025` for `task-025.md`, else the file's name without `.md`. */
  id: string;
  heading: string | null;
  modifiedMs: number;
}

export interface TaskPlan {
  title: string;
  prompt: string;
  cwd: string;
  dirs: string[];
}

export interface WorktreeDefaults {
  folder: string;
  branch: string;
}

/** What the dialog hands back: everything the core needs to open the coder tab. */
export interface TaskChoice {
  taskFile: string;
  accountId: AccountId | null;
  worktree: { folder: string; branch: string; base: string } | null;
  /** The project's prompt as the dialog left it; `null` is the default. */
  template: string | null;
}

export function taskFiles(folder: string): Promise<TaskFile[]> {
  return invoke<TaskFile[]>("task_files", { folder });
}

export function guessTaskFolder(folders: string[]): Promise<string | null> {
  return invoke<string | null>("task_folder", { folders });
}

export function defaultPrompt(): Promise<string> {
  return invoke<string>("task_default_prompt");
}

export function worktreeDefaults(
  projectFolder: string,
  taskFile: string,
): Promise<WorktreeDefaults> {
  return invoke<WorktreeDefaults>("task_worktree_defaults", {
    projectFolder,
    taskFile,
  });
}

export function planTask(request: {
  taskFile: string;
  projectFolder: string;
  worktree: [string, string] | null;
  template: string | null;
  plannerTab: string;
}): Promise<TaskPlan> {
  return invoke<TaskPlan>("task_plan", { request });
}

/** The tab a task's coder goes under: the tab itself when it is a planner, else the planner it sits under. */
export function plannerFor(tree: Tree, tab: TreeTab): TreeTab | undefined {
  if (isPlanner(tab)) return tab;
  const parent = tree.tabs.find((t) => t.id === tab.parentId);
  return parent && isPlanner(parent) ? parent : undefined;
}

/** The account the last task in the project used, if it still exists, else the tab's own, else the default. */
export function taskAccount(
  tasks: ProjectTasks | undefined,
  tab: TreeTab,
  profiles: Profiles,
): AccountId | null {
  const exists = (id: AccountId | null | undefined): id is AccountId =>
    id != null && profiles.accounts.some((a) => a.id === id);
  if (exists(tasks?.account)) return tasks.account;
  if (exists(tab.launch.account?.id)) return tab.launch.account.id;
  return profiles.defaultAccount;
}

export function withProjectTasks(
  tree: Tree,
  projectId: ProjectId,
  change: Partial<ProjectTasks>,
): Tree {
  const empty: ProjectTasks = { folder: null, prompt: null, account: null };
  return {
    ...tree,
    projects: tree.projects.map((p) =>
      p.id === projectId
        ? { ...p, tasks: { ...empty, ...p.tasks, ...change } }
        : p,
    ),
  };
}

/** The coder tab under `planner`, named for the task, in the plan's folder with the project folder added. */
export function addCoderTab(
  tree: Tree,
  planner: TreeTab,
  claude: ProgramChoice,
  account: Account | null,
  plan: TaskPlan,
): [Tree, TabId] | undefined {
  const launch = tabFor(claude, { account, cwd: plan.cwd });
  if (!launch) return undefined;
  const withDirs = { ...launch, spec: { ...launch.spec, dirs: plan.dirs } };
  const [added, id] = addTab(
    tree,
    { projectId: planner.projectId, parentId: planner.id },
    withDirs,
  );
  return [renameTab(added, id, plan.title), id];
}

/** Prompts waiting for their chat: one is typed the first time the chat reports it is idle, so after any trust prompt. */
export class Prefills {
  private readonly waiting = new Map<TabId, string>();

  set(id: TabId, text: string): void {
    this.waiting.set(id, text);
  }

  /** Types the waiting prompt, never with Enter, once the chat is ready for it. */
  reported(
    id: TabId,
    state: LiveState,
    type: ((text: string) => void) | undefined,
  ): void {
    const text = this.waiting.get(id);
    if (text === undefined || state.state !== "idle" || !type) return;
    this.waiting.delete(id);
    type(text);
  }

  forget(id: TabId): void {
    this.waiting.delete(id);
  }
}
