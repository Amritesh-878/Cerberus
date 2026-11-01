import { expect, test } from "vitest";

import type { Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import { fromSaved, type SavedTree, toSaved } from "./saved";
import { tabFor } from "./tab";
import { addProject, addTab, emptyTree, type Tree } from "./tree";

const CLAUDE: ProgramChoice = {
  id: "claude",
  label: "Claude Code",
  program: String.raw`C:\bin\claude.exe`,
  args: [],
  pastesImages: true,
  isShell: false,
};

const SHELL: ProgramChoice = {
  id: "powershell",
  label: "PowerShell",
  program: String.raw`C:\ps\powershell.exe`,
  args: ["-NoLogo"],
  pastesImages: false,
  isShell: true,
};

const PROFILES: Profiles = {
  accounts: [
    { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 1, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 1,
  shell: null,
  startFolder: null,
  revision: 0,
};

const [personal, work] = PROFILES.accounts;

function treeWithTabs(): Tree {
  const tab = (choice: ProgramChoice, account = work ?? null) => {
    const launch = tabFor(choice, { account, cwd: String.raw`C:\code\app` });
    if (!launch) throw new Error(`${choice.id} is not installed`);
    return launch;
  };
  const [withProject, app] = addProject(emptyTree(), String.raw`C:\code\app`);
  const top = { projectId: app, parentId: null };
  const [withPlanner, planner] = addTab(withProject, top, tab(CLAUDE));
  const under = { projectId: app, parentId: planner };
  const [withCoder] = addTab(withPlanner, under, tab(CLAUDE, personal));
  const [withShell] = addTab(withCoder, top, tab(SHELL));
  const setup = { projectId: null, parentId: null };
  return addTab(withShell, setup, tab(CLAUDE))[0];
}

test("a saved tree comes back with the same titles, accounts, folders and order", () => {
  const tree = treeWithTabs();

  const restored = fromSaved(toSaved(tree), PROFILES, [CLAUDE, SHELL]);

  expect(restored).toEqual({
    ...tree,
    tabs: tree.tabs.filter((t) => t.projectId !== null),
  });
});

test("setup tabs are not saved", () => {
  const saved = toSaved(treeWithTabs());

  expect(saved.tabs.map((t) => t.program)).toEqual([
    "claude",
    "claude",
    "powershell",
  ]);
});

test("only ids, names, folders and choices are saved, never a program path", () => {
  const saved = toSaved(treeWithTabs());

  expect(saved.tabs[1]).toEqual({
    id: 3,
    projectId: 1,
    parentId: 2,
    title: "Claude Code · personal",
    program: "claude",
    cwd: String.raw`C:\code\app`,
    account: 0,
  });
});

test("a renamed account shows its new name on restore", () => {
  const renamed: Profiles = {
    ...PROFILES,
    accounts: PROFILES.accounts.map((a) =>
      a.id === 1 ? { ...a, name: "isl" } : a,
    ),
  };

  const restored = fromSaved(toSaved(treeWithTabs()), renamed, [CLAUDE, SHELL]);

  expect(restored.tabs[0]?.launch.account).toEqual({ id: 1, name: "isl" });
});

test("a program no longer installed keeps its tab, which fails visibly when opened", () => {
  const saved: SavedTree = {
    projects: [{ id: 1, name: "app", folder: String.raw`C:\code\app` }],
    tabs: [
      {
        id: 2,
        projectId: 1,
        parentId: null,
        title: "old",
        program: "git-bash",
        cwd: null,
        account: 7,
      },
    ],
    nextId: 3,
  };

  const [tab] = fromSaved(saved, PROFILES, [CLAUDE]).tabs;

  expect(tab?.launch.spec).toEqual({
    program: "git-bash",
    args: [],
    cwd: null,
    account: 7,
  });
  expect(tab?.launch.account).toEqual({ id: 7, name: "removed account" });
});
