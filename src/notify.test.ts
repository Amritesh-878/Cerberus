import { expect, test } from "vitest";

import { CHOICES } from "./harness";
import { toastFor } from "./notify";
import { tabFor } from "./tab";
import { addProject, addTab, emptyTree, renameTab, type Tree } from "./tree";

const CLAUDE = CHOICES[0];
const ACCOUNT = { id: 0, name: "someone@example.com", configDir: "C:\\h" };

function treeWith(inProject: boolean): [Tree, number] {
  const [withProject, id] = addProject(emptyTree(), String.raw`C:\code\alpha`);
  const tab = CLAUDE && tabFor(CLAUDE, { account: ACCOUNT, cwd: null });
  if (!tab) throw new Error("no tab");
  return addTab(
    inProject ? withProject : emptyTree(),
    { projectId: inProject ? id : null, parentId: null },
    tab,
  );
}

test("an untouched title names the program, never the account", () => {
  const [tree, tab] = treeWith(true);

  expect(toastFor({ kind: "finished" }, tree, tab)).toEqual({
    title: "Claude Code · alpha",
    body: "Finished",
  });
});

test("a default title from before an account was renamed still drops the account", () => {
  const [tree, tab] = treeWith(true);
  const stale = {
    ...tree,
    tabs: tree.tabs.map((t) => ({
      ...t,
      title: "Claude Code · old@example.com",
    })),
  };

  expect(toastFor({ kind: "finished" }, stale, tab)?.title).toBe(
    "Claude Code · alpha",
  );
});

test("a renamed tab keeps its name, and a setup tab has no project", () => {
  const [tree, tab] = treeWith(false);

  expect(
    toastFor({ kind: "ended" }, renameTab(tree, tab, "planner"), tab),
  ).toEqual({ title: "planner", body: "The chat ended" });
});

test("a limit gives its reset time when it is known", () => {
  const [tree, tab] = treeWith(true);
  const at = new Date(2026, 10, 2, 18, 40).getTime() / 1000;

  expect(toastFor({ kind: "limited", resetsAt: null }, tree, tab)?.body).toBe(
    "Hit its limit",
  );
  expect(
    toastFor({ kind: "limited", resetsAt: at }, tree, tab, "en-US")?.body,
  ).toBe("Hit its limit, resets 6:40pm");
});

test("a tab that is gone gives no notification", () => {
  expect(toastFor({ kind: "finished" }, emptyTree(), 9)).toBeUndefined();
});
