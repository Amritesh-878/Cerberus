import { expect, test } from "vitest";

import type { Tab } from "./tab";
import {
  addProject,
  addTab,
  closeTab,
  defaultTitle,
  emptyTree,
  folderName,
  groups,
  moveTab,
  neighbour,
  planners,
  removeProject,
  renameProject,
  renameTab,
  reuseRows,
  stepTab,
  type Tree,
} from "./tree";

function launch(program: string, account: string | null = "Work"): Tab {
  return {
    program,
    label: program === "claude" ? "Claude Code" : "PowerShell",
    spec: {
      program,
      args: [],
      cwd: null,
      account: account === null ? null : 1,
    },
    pastesImages: program === "claude",
    account: account === null ? null : { id: 1, name: account },
    input: null,
    signIn: false,
  };
}

// alpha: planner 2 (coders 3 and 4) and shell 5; beta: planner 7.
function sample(): Tree {
  let tree = emptyTree();
  [tree] = addProject(tree, String.raw`C:\code\alpha`);
  [tree] = addTab(tree, { projectId: 1, parentId: null }, launch("claude"));
  [tree] = addTab(tree, { projectId: 1, parentId: 2 }, launch("claude"));
  [tree] = addTab(tree, { projectId: 1, parentId: 2 }, launch("claude"));
  [tree] = addTab(tree, { projectId: 1, parentId: null }, launch("powershell"));
  [tree] = addProject(tree, "C:/code/beta/");
  [tree] = addTab(tree, { projectId: 6, parentId: null }, launch("claude"));
  return tree;
}

const ids = (tree: Tree, search = "") =>
  groups(tree, search, (t) => t.launch.account?.name ?? "").map((g) =>
    g.rows.map((r) => (r.sub ? -r.tab.id : r.tab.id)),
  );

test("a project is named after its folder", () => {
  expect(folderName(String.raw`C:\code\alpha`)).toBe("alpha");
  expect(folderName("C:/code/beta/")).toBe("beta");
  expect(folderName("D:\\")).toBe("D:");
});

test("adding a folder twice keeps one project", () => {
  const tree = sample();

  const [same, id] = addProject(tree, String.raw`c:\CODE\alpha`);

  expect(id).toBe(1);
  expect(same).toBe(tree);
});

test("coder sub-tabs list under their planner", () => {
  expect(ids(sample())).toEqual([[2, -3, -4, 5], [7]]);
});

test("the default title is the program and the account", () => {
  expect(defaultTitle(launch("claude"))).toBe("Claude Code · Work");
  expect(defaultTitle(launch("powershell", null))).toBe("PowerShell");
  expect(sample().tabs[0]?.title).toBe("Claude Code · Work");
});

test("only a Claude Code tab at the top of the same project takes sub-tabs", () => {
  const tree = sample();

  const [underShell] = addTab(
    tree,
    { projectId: 1, parentId: 5 },
    launch("claude"),
  );
  const [underCoder] = addTab(
    tree,
    { projectId: 1, parentId: 3 },
    launch("claude"),
  );
  const [otherProject] = addTab(
    tree,
    { projectId: 6, parentId: 2 },
    launch("claude"),
  );

  for (const added of [underShell, underCoder, otherProject]) {
    expect(added.tabs.at(-1)?.parentId).toBeNull();
  }
  expect(planners(tree, 1).map((t) => t.id)).toEqual([2]);
});

test("rename trims the title and ignores a blank one", () => {
  const renamed = renameTab(sample(), 3, "  Login form ");

  expect(renamed.tabs.find((t) => t.id === 3)?.title).toBe("Login form");
  expect(renameTab(renamed, 3, "   ")).toBe(renamed);
});

test("rename keeps every other tab the same object", () => {
  const tree = sample();

  const renamed = renameTab(tree, 3, "x");

  expect(
    renamed.tabs.filter((t, i) => t !== tree.tabs[i]).map((t) => t.id),
  ).toEqual([3]);
});

test("closing a planner moves its sub-tabs up instead of closing them", () => {
  expect(ids(closeTab(sample(), 2))).toEqual([[3, 4, 5], [7]]);
});

test("closing a sub-tab leaves its planner and the other sub-tab", () => {
  expect(ids(closeTab(sample(), 3))).toEqual([[2, -4, 5], [7]]);
});

test("a sub-tab moves to the top and back under a planner", () => {
  const top = moveTab(sample(), 4, null);
  expect(ids(top)).toEqual([[2, -3, 4, 5], [7]]);

  expect(ids(moveTab(top, 4, 2))).toEqual([[2, -3, -4, 5], [7]]);
});

test("a move that would break the tree is ignored", () => {
  const tree = sample();

  expect(moveTab(tree, 2, 7)).toBe(tree);
  expect(moveTab(tree, 3, 5)).toBe(tree);
  expect(moveTab(tree, 3, 3)).toBe(tree);
  expect(moveTab(tree, 99, null)).toBe(tree);
});

test("a search keeps the planner of a matching sub-tab and drops empty projects", () => {
  const tree = renameTab(sample(), 4, "API tests");

  expect(ids(tree, "api")).toEqual([[2, -4]]);
  expect(ids(tree, "work")).toEqual([[2, -3, -4, 5], [7]]);
  expect(ids(tree, "nothing")).toEqual([]);
});

test("setup tabs list first, outside any project", () => {
  const [tree] = addTab(
    sample(),
    { projectId: null, parentId: 2 },
    launch("claude"),
  );

  const listed = groups(tree, "", () => "");

  expect(listed[0]?.project).toBeNull();
  expect(listed[0]?.rows.map((r) => [r.tab.id, r.sub])).toEqual([[8, false]]);
});

test("the tree line stops at a planner's last sub-tab", () => {
  const rows = groups(sample(), "", () => "")[0]?.rows ?? [];

  expect(rows.map((r) => [r.tab.id, r.last])).toEqual([
    [2, false],
    [3, false],
    [4, true],
    [5, false],
  ]);
});

test("after a close the next tab shows, or the one before at the end", () => {
  const tree = sample();

  expect(neighbour(tree, 2)).toBe(3);
  expect(neighbour(tree, 7)).toBe(5);
  expect(neighbour(closeTab(tree, 7), 99)).toBeUndefined();
});

test("stepping through tabs follows the sidebar and wraps", () => {
  const tree = sample();

  expect(stepTab(tree, 5, 1)).toBe(7);
  expect(stepTab(tree, 7, 1)).toBe(2);
  expect(stepTab(tree, 2, -1)).toBe(7);
  expect(stepTab(tree, undefined, 1)).toBe(2);
  expect(stepTab(emptyTree(), undefined, 1)).toBeUndefined();
});

test("a rebuild keeps every row whose tab did not change", () => {
  const tree = sample();
  const before = groups(tree, "", () => "");
  const renamed = renameTab(tree, 3, "Login form");

  const after = reuseRows(
    before,
    groups(renamed, "", () => ""),
  );

  const alpha = after[0]?.rows ?? [];
  expect(alpha.map((r) => r === before[0]?.rows[alpha.indexOf(r)])).toEqual([
    true,
    false,
    true,
    true,
  ]);
  expect(after[1]).toBe(before[1]);
});

test("a rebuild with nothing changed returns the same groups", () => {
  const tree = sample();
  const before = groups(tree, "", () => "");

  const after = reuseRows(
    before,
    groups(tree, "", () => "x"),
  );

  expect(after.every((g, i) => g === before[i])).toBe(true);
});

test("a project rename trims the name and ignores a blank one", () => {
  const renamed = renameProject(sample(), 1, "  Alpha app ");

  expect(renamed.projects.map((p) => p.name)).toEqual(["Alpha app", "beta"]);
  expect(renamed.projects[0]?.folder).toBe(String.raw`C:\code\alpha`);
  expect(renameProject(renamed, 1, "  ")).toBe(renamed);
});

test("removing a project forgets it and its tabs and nothing else", () => {
  const removed = removeProject(sample(), 1);

  expect(removed.projects.map((p) => p.name)).toEqual(["beta"]);
  expect(removed.tabs.map((t) => t.id)).toEqual([7]);
});

test("removing a project keeps setup tabs", () => {
  const [tree] = addTab(
    sample(),
    { projectId: null, parentId: null },
    launch("claude"),
  );

  expect(removeProject(tree, 1).tabs.map((t) => t.id)).toEqual([7, 8]);
});
