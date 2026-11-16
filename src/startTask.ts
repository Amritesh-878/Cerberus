import { invoke } from "@tauri-apps/api/core";

import type { Account, AccountId, Profiles } from "./accounts";
import type { LiveState } from "./board";
import type { ProgramChoice } from "./pty";
import { tabFor } from "./tab";
import {
  addTab,
  isPlanner,
  setHandoff,
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
  taskId: string;
  handoffFile: string;
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

export interface Branches {
  /** The repo's default branch, the base a new worktree starts from unless changed. */
  default: string | null;
  names: string[];
}

export function taskBranches(folder: string): Promise<Branches> {
  return invoke<Branches>("task_branches", { folder });
}

/** Runs `git worktree add`; a failure rejects with git's own message. */
export function createWorktree(
  repo: string,
  worktree: { folder: string; branch: string; base: string },
): Promise<void> {
  return invoke("task_worktree", { repo, ...worktree });
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

/** The coder tab under `planner`, named for the task, in the plan's folder with the project folder added; it watches for its handoff from `now`. */
export function addCoderTab(
  tree: Tree,
  planner: TreeTab,
  claude: ProgramChoice,
  account: Account | null,
  plan: TaskPlan,
  now = Date.now(),
): [Tree, TabId] | undefined {
  const launch = tabFor(claude, { account, cwd: plan.cwd });
  if (!launch) return undefined;
  const withDirs = { ...launch, spec: { ...launch.spec, dirs: plan.dirs } };
  const [added, id] = addTab(
    tree,
    { projectId: planner.projectId, parentId: planner.id },
    withDirs,
  );
  const handoff = {
    taskId: plan.taskId,
    file: plan.handoffFile,
    since: now,
    state: "working" as const,
  };
  return [setHandoff(renameTab(added, id, plan.title), id, handoff), id];
}

export interface TaskDeps {
  tree: () => Tree;
  setTree: (tree: Tree) => void;
  profiles: () => Profiles | undefined;
  claude: () => ProgramChoice | undefined;
  prefills: Prefills;
  select: (id: TabId) => void;
}

/** Makes the worktree if asked, then the coder sub-tab with its prompt waiting; a failure rejects and opens nothing. */
export async function openTask(
  deps: TaskDeps,
  from: TabId,
  choice: TaskChoice,
): Promise<void> {
  const tab = deps.tree().tabs.find((t) => t.id === from);
  const planner = tab && plannerFor(deps.tree(), tab);
  const project = deps.tree().projects.find((p) => p.id === planner?.projectId);
  if (!planner || !project) throw new Error("The planner tab is gone.");
  const claude = deps.claude();
  if (!claude) throw new Error("Claude Code isn't installed.");
  if (choice.worktree) await createWorktree(project.folder, choice.worktree);
  const plan = await planTask({
    taskFile: choice.taskFile,
    projectFolder: project.folder,
    worktree: choice.worktree && [
      choice.worktree.folder,
      choice.worktree.branch,
    ],
    template: choice.template,
    plannerTab: planner.title,
  });
  const account =
    deps.profiles()?.accounts.find((a) => a.id === choice.accountId) ?? null;
  // Read again, since the tree may have changed while git ran.
  const now = deps.tree().tabs.find((t) => t.id === planner.id);
  const added = now && addCoderTab(deps.tree(), now, claude, account, plan);
  if (!added) throw new Error("The planner tab is gone.");
  const [tree, id] = added;
  deps.setTree(tree);
  deps.prefills.set(id, plan.prompt);
  deps.select(id);
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
