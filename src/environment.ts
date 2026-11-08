import { invoke } from "@tauri-apps/api/core";
import { batch } from "solid-js";

import { renameTab, setFolders, type TabId, type Tree } from "./tree";

/** A value sets the variable for the tab's next start; `null` removes it. */
export interface EnvEdit {
  name: string;
  value: string | null;
}

/** What the Edit environment dialog saves. */
export interface EnvironmentChange {
  title: string;
  cwd: string | null;
  dirs: string[];
  env: EnvEdit[];
}

/** The tab's variable names; the values stay in the core. */
export function tabEnv(tab: TabId): Promise<string[]> {
  return invoke<string[]>("tab_env", { tab });
}

/** All edits or none; the core refuses bad and managed names. */
export function tabSetEnv(tab: TabId, edits: EnvEdit[]): Promise<string[]> {
  return invoke<string[]>("tab_set_env", { tab, edits });
}

/** Copies a stopped tab's chat to the folder's project, so the next start resumes it there. */
export function tabMoveFolder(tab: TabId, folder: string): Promise<void> {
  return invoke("tab_move_folder", { tab, folder });
}

/** Typed into a running chat for the user to send: control characters dropped, so never an Enter. */
export function addDirCommand(folder: string): string {
  return `/add-dir ${folder.replace(/\p{Cc}/gu, "")}`;
}

export interface FolderPlan {
  restart: boolean;
  added: string[];
  removed: string[];
}

export function planFolders(
  before: { cwd: string | null; dirs: readonly string[] },
  after: { cwd: string | null; dirs: readonly string[] },
): FolderPlan {
  return {
    restart: before.cwd !== after.cwd,
    added: after.dirs.filter((d) => !before.dirs.includes(d)),
    removed: before.dirs.filter((d) => !after.dirs.includes(d)),
  };
}

export interface EnvironmentDeps {
  tree: () => Tree;
  setTree: (tree: Tree) => void;
  runOf: (id: TabId) => number;
  setRun: (id: TabId, run: number) => void;
  /** Types into the tab's running Claude Code chat; does nothing for a shell or a stopped chat. */
  typeInto: (id: TabId, text: string) => void;
  setAlert: (id: TabId, alert: string | undefined) => void;
}

/** Saves a dialog's change: variables first, so a refusal changes nothing else; a new main folder restarts the tab there. */
export async function applyEnvironment(
  deps: EnvironmentDeps,
  id: TabId,
  change: EnvironmentChange,
): Promise<void> {
  const tab = deps.tree().tabs.find((t) => t.id === id);
  if (!tab) return;
  if (change.env.length > 0) await tabSetEnv(id, change.env);
  const spec = tab.launch.spec;
  const plan = planFolders({ cwd: spec.cwd, dirs: spec.dirs ?? [] }, change);
  deps.setTree(renameTab(deps.tree(), id, change.title));
  if (!plan.restart) {
    deps.setTree(setFolders(deps.tree(), id, change.cwd, change.dirs));
    const first = plan.added[0];
    if (first !== undefined) deps.typeInto(id, addDirCommand(first));
    return;
  }
  const run = deps.runOf(id);
  batch(() => {
    deps.setRun(id, 0);
    deps.setAlert(id, undefined);
  });
  const folder = change.cwd ?? "";
  try {
    await tabMoveFolder(id, folder);
    deps.setTree(setFolders(deps.tree(), id, change.cwd, change.dirs));
  } catch (error) {
    // The folder stays as it was; the extra folders still apply from the next start.
    const now = deps.tree().tabs.find((t) => t.id === id);
    deps.setTree(
      setFolders(deps.tree(), id, now?.launch.spec.cwd ?? null, change.dirs),
    );
    deps.setAlert(id, `Could not move the chat: ${String(error)}`);
  }
  deps.setRun(id, run + 1);
}
