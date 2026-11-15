import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import {
  activeTitle,
  button,
  cleanup,
  click,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { Changes, Removed, Worktree, Written } from "./panelData";
import type { SavedTree } from "./saved";
import { refusalText, removedText, worktreeDetail } from "./Worktrees";

afterEach(cleanup);

const ALPHA = String.raw`C:\code\alpha`;
const WT = String.raw`C:\code\alpha-wt`;
const MERGED = String.raw`C:\code\alpha-merged`;
const CLOSED = String.raw`C:\code\alpha-closed`;
const DIRTY = String.raw`C:\code\alpha-dirty`;
const USED = String.raw`C:\code\alpha-used`;

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: ALPHA }],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: ALPHA,
      account: 1,
      dirs: [],
    },
    {
      id: 4,
      projectId: 1,
      parentId: 3,
      title: "coder",
      program: "powershell",
      cwd: USED,
      account: 1,
      dirs: [],
    },
  ],
  nextId: 5,
};

const PROFILES: Profiles = {
  ...SAVED,
  panel: { ...SAVED.panel, open: true },
};

function repo(name: string, root: string, branch: string): Changes {
  return {
    kind: "repo",
    name,
    root,
    branch,
    commit: null,
    base: "origin/main",
    added: 2,
    removed: 0,
    gh: false,
    vscode: true,
  };
}

function worktree(path: string, overrides: Partial<Worktree> = {}): Worktree {
  return {
    path,
    name: path.split("\\").at(-1) ?? path,
    branch: "feature",
    commit: null,
    main: false,
    dirty: false,
    missing: false,
    locked: false,
    pr: null,
    gone: false,
    tabs: [],
    stale: false,
    refusal: null,
    ...overrides,
  };
}

const WORKTREES: Worktree[] = [
  worktree(ALPHA, { branch: "main", main: true, refusal: "main" }),
  worktree(MERGED, {
    branch: "feat/merged",
    pr: { number: 12, state: "merged" },
    stale: true,
  }),
  worktree(CLOSED, {
    branch: "feat/closed",
    pr: { number: 9, state: "closed" },
    stale: true,
  }),
  worktree(DIRTY, {
    branch: "feat/dirty",
    pr: { number: 7, state: "merged" },
    dirty: true,
    refusal: "dirty",
  }),
  worktree(USED, {
    branch: "feat/used",
    pr: { number: 5, state: "open" },
    tabs: [4],
    refusal: "inUse",
  }),
];

function worktreeCore() {
  const pty = ptyCore();
  const calls: { cmd: string; args: unknown }[] = [];
  const writes: string[] = [];
  const saves: SavedTree[] = [];
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  const written: Written[] = [
    { folder: WT, changes: repo("alpha-wt", "C:/code/alpha-wt", "feat/wt") },
  ];
  return {
    pty,
    calls,
    writes,
    saves,
    named: (cmd: string) => calls.filter((c) => c.cmd === cmd),
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null, ended: [] }),
      tree_save: (args: unknown) => {
        saves.push(field(args, "tree") as SavedTree);
      },
      pty_write: (args: unknown) => {
        writes.push(String(field(args, "data")));
      },
      panel_changes: () => repo("alpha", "C:/code/alpha", "main"),
      panel_prs: () => [],
      panel_connectors: () => ({ servers: [], unreadable: [] }),
      panel_skills: () => [],
      ...record("panel_written", () => written),
      ...record("panel_worktrees", () => WORKTREES),
      ...record("panel_remove_worktrees", (args): Removed[] =>
        (field(args, "paths") as string[]).map((path) => ({
          path,
          name: path.split("\\").at(-1) ?? path,
          refusal: null,
          branch: path === MERGED ? "feat/merged" : "feat/closed",
          branchDeleted: path === MERGED,
        })),
      ),
      ...record("panel_open_vscode", () => null),
      ...record("panel_open_folder", () => null),
    },
  };
}

function changesCard(root: HTMLElement): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".card")].find(
    (c) => c.querySelector(".card-title")?.textContent === "Changes",
  );
  if (!found) throw new Error("no Changes card");
  return found;
}

function repoRow(root: HTMLElement, name: string): HTMLElement {
  const found = [
    ...changesCard(root).querySelectorAll<HTMLElement>(".repo-row"),
  ].find((r) => r.querySelector(".row-main")?.textContent === name);
  if (!found) throw new Error(`no ${name} repo row`);
  return found;
}

function worktreeRow(root: HTMLElement, name: string): HTMLElement {
  const found = [
    ...changesCard(root).querySelectorAll<HTMLElement>(".worktree-row"),
  ].find((r) => r.querySelector(".row-main")?.textContent.startsWith(name));
  if (!found) throw new Error(`no ${name} worktree row`);
  return found;
}

async function showWorktrees(root: HTMLElement): Promise<void> {
  click(repoRow(root, "alpha"), "More for alpha");
  await settle();
  click(repoRow(root, "alpha"), "Worktrees");
  await settle();
}

test("a repo the chat wrote into shows in Changes, marked, with its branch and Open in VS Code", async () => {
  const core = worktreeCore();
  const root = await renderApp(() => PROFILES, core.core);
  await open(root, "planner");

  const row = repoRow(root, "alpha-wt");
  expect(row.textContent).toContain("written by this chat");
  expect(row.textContent).toContain("feat/wt");
  expect(row.textContent).toContain("+2");
  expect(repoRow(root, "alpha").textContent).not.toContain(
    "written by this chat",
  );
  expect(field(core.named("panel_written")[0]?.args, "tab")).toBe(3);

  click(row, "Open in VS Code");
  await settle();

  expect(core.named("panel_open_vscode").map((c) => c.args)).toEqual([
    { folder: WT },
  ]);
});

test("Add to the tab saves the written repo as the tab's folder and types /add-dir, never Enter", async () => {
  const core = worktreeCore();
  const root = await renderApp(() => PROFILES, core.core);
  await open(root, "planner");

  click(repoRow(root, "alpha"), "More for alpha");
  await settle();
  expect(() => button(repoRow(root, "alpha"), "Add to the tab")).toThrow();
  click(repoRow(root, "alpha-wt"), "More for alpha-wt");
  await settle();
  click(repoRow(root, "alpha-wt"), "Add to the tab");
  await settle();

  expect(core.saves.at(-1)?.tabs.find((t) => t.id === 3)?.dirs).toEqual([WT]);
  expect(core.writes).toEqual([`/add-dir ${WT}`]);
});

test("the worktree list shows each branch, PR state, uncommitted work and the tabs in it, with stale ones marked", async () => {
  const core = worktreeCore();
  const root = await renderApp(() => PROFILES, core.core);
  await open(root, "planner");

  await showWorktrees(root);

  expect(core.named("panel_worktrees").map((c) => c.args)).toEqual([
    { tab: 3, folder: ALPHA },
  ]);
  expect(worktreeRow(root, "alpha").textContent).toContain("main");
  expect(worktreeRow(root, "alpha-merged").textContent).toContain(
    "feat/merged · #12 merged",
  );
  expect(worktreeRow(root, "alpha-merged").classList.contains("stale")).toBe(
    true,
  );
  expect(worktreeRow(root, "alpha-dirty").textContent).toContain(
    "uncommitted changes",
  );
  expect(worktreeRow(root, "alpha-used").textContent).toContain(
    "#5 open · in coder",
  );
  expect(worktreeRow(root, "alpha-used").classList.contains("stale")).toBe(
    false,
  );
});

test("Remove stale asks first with the list and removes only after the confirm, only the stale ones", async () => {
  const core = worktreeCore();
  const root = await renderApp(() => PROFILES, core.core);
  await open(root, "planner");
  await showWorktrees(root);

  click(changesCard(root), "Remove stale (2)");
  await settle();
  const confirm = () => changesCard(root).querySelector(".worktree-confirm");
  expect(confirm()?.textContent).toContain("alpha-merged");
  expect(confirm()?.textContent).toContain("alpha-closed");
  expect(confirm()?.textContent).not.toContain("alpha-dirty");
  expect(core.named("panel_remove_worktrees")).toEqual([]);
  click(changesCard(root), "Cancel");
  await settle();
  expect(confirm()).toBeNull();
  expect(core.named("panel_remove_worktrees")).toEqual([]);

  click(changesCard(root), "Remove stale (2)");
  await settle();
  click(changesCard(root), "Remove");
  await settle();

  expect(core.named("panel_remove_worktrees").map((c) => c.args)).toEqual([
    { tab: 3, folder: ALPHA, paths: [MERGED, CLOSED] },
  ]);
  const text = changesCard(root).textContent;
  expect(text).toContain(
    "alpha-merged: removed, merged branch feat/merged deleted here",
  );
  expect(text).toContain("alpha-closed: removed, branch feat/closed kept");
  // The list is read again after the removal.
  expect(core.named("panel_worktrees")).toHaveLength(2);
});

test("Remove on a dirty or in-use worktree says why and asks the core for nothing", async () => {
  const core = worktreeCore();
  const root = await renderApp(() => PROFILES, core.core);
  await open(root, "planner");
  await showWorktrees(root);

  click(worktreeRow(root, "alpha-dirty"), "More for alpha-dirty");
  await settle();
  click(worktreeRow(root, "alpha-dirty"), "Remove…");
  await settle();
  expect(changesCard(root).textContent).toContain(
    "alpha-dirty can't be removed: it has uncommitted changes.",
  );
  click(changesCard(root), "OK");
  await settle();
  click(worktreeRow(root, "alpha-used"), "More for alpha-used");
  await settle();
  click(worktreeRow(root, "alpha-used"), "Remove…");
  await settle();

  expect(changesCard(root).textContent).toContain(
    "alpha-used can't be removed: a tab runs in it (coder).",
  );
  expect(() => button(changesCard(root), "Remove")).toThrow();
  expect(core.named("panel_remove_worktrees")).toEqual([]);
});

test("a worktree opens in VS Code, the file manager, or a new shell or Claude tab there", async () => {
  const core = worktreeCore();
  const root = await renderApp(() => PROFILES, core.core);
  await open(root, "planner");
  await showWorktrees(root);
  const pick = async (item: string) => {
    click(worktreeRow(root, "alpha-merged"), "More for alpha-merged");
    await settle();
    click(worktreeRow(root, "alpha-merged"), item);
    await settle();
  };

  await pick("Open in VS Code");
  await pick("Show in file manager");
  expect(core.named("panel_open_vscode").map((c) => c.args)).toEqual([
    { folder: MERGED },
  ]);
  expect(core.named("panel_open_folder").map((c) => c.args)).toEqual([
    { folder: MERGED },
  ]);
  await pick("New shell tab here");
  expect(activeTitle(root)).toBe("alpha-merged");
  const shell = field(core.pty.spawned.at(-1), "request");
  expect(field(shell, "cwd")).toBe(MERGED);
  expect(field(shell, "program")).toBe(String.raw`C:\ps\powershell.exe`);

  await open(root, "planner");
  await showWorktrees(root);
  await pick("New Claude tab here");
  const claude = field(core.pty.spawned.at(-1), "request");
  expect(field(claude, "cwd")).toBe(MERGED);
  expect(field(claude, "program")).toBe(String.raw`C:\bin\claude.exe`);
});

test("refusals and results read plainly", () => {
  expect(refusalText("inUse", ["coder", "planner"])).toBe(
    "a tab runs in it (coder, planner)",
  );
  expect(refusalText("locked")).toBe("git has it locked");
  expect(
    worktreeDetail(
      worktree(MERGED, {
        branch: null,
        commit: "abc1234",
        gone: true,
        missing: true,
      }),
      [],
    ),
  ).toBe("detached at abc1234 · no PR · remote branch gone · folder gone");
  expect(
    removedText({
      path: DIRTY,
      name: "alpha-dirty",
      refusal: "dirty",
      branch: "feat/dirty",
      branchDeleted: false,
    }),
  ).toBe("alpha-dirty: kept, it has uncommitted changes");
});

test("a written worktree of the same GitHub repo does not list that repo's PRs again", async () => {
  const core = worktreeCore();
  const prs = [
    {
      number: 14,
      title: "Invoices",
      state: "open",
      key: "example/alpha#14",
      updated: "u14",
    },
  ];
  const root = await renderApp(() => PROFILES, {
    ...core.core,
    panel_prs: () => prs,
    panel_changes: () => ({
      ...repo("alpha", "C:/code/alpha", "main"),
      gh: true,
    }),
    panel_written: () => [
      {
        folder: WT,
        changes: {
          ...repo("alpha-wt", "C:/code/alpha-wt", "feat/wt"),
          gh: true,
        },
      },
    ],
  });
  await open(root, "planner");

  const titles = [...changesCard(root).querySelectorAll(".pr-row")].map(
    (r) => r.textContent,
  );
  expect(titles).toHaveLength(1);
  expect(titles[0]).toContain("alpha #14");
});
