import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AccountId, Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import { installed, type Tab } from "./tab";
import type { Project, ProjectId, TabId, Tree, TreeTab } from "./tree";

export interface SavedTab {
  id: TabId;
  projectId: ProjectId;
  parentId: TabId | null;
  title: string;
  program: string;
  cwd: string | null;
  account: AccountId | null;
}

export interface SavedTree {
  projects: Project[];
  tabs: SavedTab[];
  nextId: number;
}

export function loadTree(): Promise<SavedTree> {
  return invoke<SavedTree>("tree_load");
}

export function saveTree(tree: SavedTree): Promise<void> {
  return invoke("tree_save", { tree });
}

/** Hears each change in whether the disk holds the latest tree: the error, or `null` once it does. */
export function onWriteStatus(
  handler: (error: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>("tree-save", (event) => {
    handler(event.payload);
  });
}

/** Setup tabs, such as signing an account in, are left out because they belong to that moment only. */
export function toSaved(tree: Tree): SavedTree {
  const tabs = tree.tabs.flatMap((tab): SavedTab[] =>
    tab.projectId === null
      ? []
      : [
          {
            id: tab.id,
            projectId: tab.projectId,
            parentId: tab.parentId,
            title: tab.title,
            program: tab.launch.program,
            cwd: tab.launch.spec.cwd,
            account: tab.launch.account?.id ?? null,
          },
        ],
  );
  return { projects: tree.projects, tabs, nextId: tree.nextId };
}

// Looked up again so an updated install is used, and a missing program fails visibly in its tab.
function launchOf(
  saved: SavedTab,
  profiles: Profiles,
  programs: readonly ProgramChoice[],
): Tab {
  const choice = installed(programs, saved.program);
  const account =
    saved.account === null
      ? null
      : {
          id: saved.account,
          name:
            profiles.accounts.find((a) => a.id === saved.account)?.name ??
            "removed account",
        };
  return {
    program: saved.program,
    label: choice?.label ?? saved.program,
    spec: {
      program: choice?.program ?? saved.program,
      args: choice?.args ?? [],
      cwd: saved.cwd,
      account: saved.account,
    },
    pastesImages: choice?.pastesImages ?? false,
    account,
    input: null,
    signIn: false,
  };
}

export function fromSaved(
  saved: SavedTree,
  profiles: Profiles,
  programs: readonly ProgramChoice[],
): Tree {
  const tabs = saved.tabs.map((tab): TreeTab => ({
    id: tab.id,
    projectId: tab.projectId,
    parentId: tab.parentId,
    title: tab.title,
    launch: launchOf(tab, profiles, programs),
  }));
  return { projects: saved.projects, tabs, nextId: saved.nextId };
}
