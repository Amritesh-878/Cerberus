import { afterEach, expect, test, vi } from "vitest";

import type { PanelPrefs, Profiles } from "./accounts";
import {
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
import type { Changes, PrRow } from "./panelData";
import { POLL_MS } from "./panelFeed";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const ALPHA = String.raw`C:\code\alpha`;
const PLANS = String.raw`C:\code\plans`;

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
      dirs: [PLANS],
    },
  ],
  nextId: 4,
};

function repo(name: string, root: string, branch: string): Changes {
  return {
    kind: "repo",
    name,
    root,
    branch,
    commit: null,
    base: "origin/main",
    added: 3,
    removed: 1,
    gh: true,
    vscode: false,
  };
}

function pr(number: number, state: PrRow["state"], repoName = "alpha"): PrRow {
  return {
    number,
    title: `Change ${String(number)}`,
    state,
    key: `example/${repoName}#${String(number)}`,
    updated: `u${String(number)}`,
  };
}

const STATES: PrRow["state"][] = ["draft", "open", "merged", "closed"];
// Newest first, as the core sends them: 12 down to 1.
const ALPHA_PRS = Array.from({ length: 12 }, (_, i) =>
  pr(12 - i, STATES[i % 4] ?? "open"),
);

function prsCore(panel: Partial<PanelPrefs> = {}, mainGone = false) {
  const pty = ptyCore();
  const calls: { cmd: string; args: unknown }[] = [];
  const saved: PanelPrefs[] = [];
  let profiles: Profiles = {
    ...SAVED,
    panel: { ...SAVED.panel, open: true, dismissed: {}, ...panel },
  };
  let alphaPrs = ALPHA_PRS;
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  return {
    calls,
    saved,
    profiles: () => profiles,
    setAlphaPrs: (prs: PrRow[]) => {
      alphaPrs = prs;
    },
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null, ended: [] }),
      set_panel: (args: unknown) => {
        const next = field(args, "panel") as PanelPrefs;
        saved.push(next);
        profiles = {
          ...profiles,
          panel: next,
          revision: profiles.revision + 1,
        };
        return profiles;
      },
      ...record("panel_changes", (args) => {
        if (field(args, "folder") !== ALPHA)
          return repo("plans", PLANS, "main");
        if (mainGone) throw new Error("folder not found");
        return repo("alpha", ALPHA, "feature");
      }),
      ...record("panel_prs", (args) =>
        field(args, "folder") === ALPHA ? alphaPrs : [pr(4, "open", "plans")],
      ),
      panel_connectors: () => ({ servers: [], unreadable: [] }),
      panel_skills: () => [],
      ...record("panel_open_pr", () => null),
      ...record(
        "panel_pr_link",
        () => "https://github.com/example/alpha/pull/12",
      ),
      ...record("panel_pr_ready", () => null),
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

function prTitles(root: HTMLElement): string[] {
  return [...changesCard(root).querySelectorAll(".pr-row .row-main")].map(
    (row) => row.textContent,
  );
}

function prRow(root: HTMLElement, number: number): HTMLElement {
  const found = [
    ...changesCard(root).querySelectorAll<HTMLElement>(".pr-row"),
  ].find((row) => row.textContent.includes(`#${String(number)}`));
  if (!found) throw new Error(`no #${String(number)} row`);
  return found;
}

async function menu(root: HTMLElement, number: number): Promise<HTMLElement> {
  click(prRow(root, number), `More for #${String(number)}`);
  await settle();
  const found = prRow(root, number).querySelector<HTMLElement>(".pr-menu");
  if (!found) throw new Error("no menu");
  return found;
}

test("each folder's repo has a row and its PRs, five at first with every state named", async () => {
  const core = prsCore();
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  const text = changesCard(root).textContent;
  expect(text).toContain("alpha");
  expect(text).toContain("plans");
  const createPr = [...changesCard(root).querySelectorAll("button")].filter(
    (b) => b.textContent === "Create PR",
  );
  expect(createPr).toHaveLength(2);
  expect(prTitles(root)).toEqual([
    "Change 12",
    "Change 11",
    "Change 10",
    "Change 9",
    "Change 8",
    "Change 4",
  ]);
  expect(prRow(root, 12).textContent).toContain("Draft");
  expect(prRow(root, 11).textContent).toContain("Open");
  expect(prRow(root, 10).textContent).toContain("Merged");
  expect(prRow(root, 9).textContent).toContain("Closed");
  expect(prRow(root, 4).textContent).toContain("plans #4");
});

test("Show more adds five rows at a time until all are shown", async () => {
  const core = prsCore();
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  click(changesCard(root), "Show more");
  await settle();
  expect(prTitles(root)).toHaveLength(11);
  click(changesCard(root), "Show more");
  await settle();

  expect(prTitles(root)).toHaveLength(13);
  expect([...changesCard(root).querySelectorAll(".show-more")]).toEqual([]);
});

test("Dismiss hides the PR and saves it until the PR changes", async () => {
  const core = prsCore();
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  click(await menu(root, 11), "Dismiss");
  await settle();

  expect(prTitles(root)).not.toContain("Change 11");
  expect(prTitles(root)).toContain("Change 7");
  expect(core.saved.at(-1)?.dismissed).toEqual({ "example/alpha#11": "u11" });

  // Updated on GitHub: the next read brings it back.
  core.setAlphaPrs(
    ALPHA_PRS.map((p) => (p.number === 11 ? { ...p, updated: "u11b" } : p)),
  );
  await open(root, "planner");
  click(root, "Right panel");
  await settle();
  click(root, "Right panel");
  await settle();
  expect(prTitles(root)).toContain("Change 11");
});

test("Open on GitHub and Copy link go through the core by number", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
  const core = prsCore();
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  const items = [
    ...(await menu(root, 12)).querySelectorAll('[role="menuitem"]'),
  ].map((b) => b.textContent.trim());
  // Open has its own button on the row, so the menu keeps the rest.
  expect(items).toEqual(["Copy link", "Ready for review…", "Dismiss"]);
  click(prRow(root, 12), "Copy link");
  await settle();
  click(prRow(root, 11), "Open #11 on GitHub");
  await settle();

  const args = (cmd: string) =>
    core.calls
      .filter((c) => c.cmd === cmd)
      .map((c) => [field(c.args, "folder"), field(c.args, "number")]);
  expect(args("panel_pr_link")).toEqual([[ALPHA, 12]]);
  expect(writeText).toHaveBeenCalledWith(
    "https://github.com/example/alpha/pull/12",
  );
  expect(args("panel_open_pr")).toEqual([[ALPHA, 11]]);
});

test("Ready for review shows only on a draft and runs only after the confirm", async () => {
  const core = prsCore();
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  expect((await menu(root, 11)).textContent).not.toContain("Ready for review");
  click(prRow(root, 11), "More for #11");
  await settle();

  const draft = await menu(root, 12);
  click(draft, "Ready for review…");
  await settle();
  expect(core.calls.map((c) => c.cmd)).not.toContain("panel_pr_ready");
  expect(prRow(root, 12).textContent).toContain(
    "Mark #12 ready for review on GitHub?",
  );
  click(prRow(root, 12), "Cancel");
  await settle();
  expect(core.calls.map((c) => c.cmd)).not.toContain("panel_pr_ready");

  click(await menu(root, 12), "Ready for review…");
  await settle();
  const before = core.calls.filter((c) => c.cmd === "panel_prs").length;
  click(prRow(root, 12), "Mark ready");
  await settle();

  const ready = core.calls.filter((c) => c.cmd === "panel_pr_ready");
  expect(
    ready.map((c) => [field(c.args, "folder"), field(c.args, "number")]),
  ).toEqual([[ALPHA, 12]]);
  // The list is read again so the row shows the new state.
  expect(
    core.calls.filter((c) => c.cmd === "panel_prs").length,
  ).toBeGreaterThan(before);
  expect(button(root, "More for #12")).toBeTruthy();
});

test("a poll while the confirm is open keeps it open", async () => {
  vi.useFakeTimers({ toFake: ["setInterval"] });
  try {
    const core = prsCore();
    const root = await renderApp(core.profiles, core.core);
    await open(root, "planner");
    click(await menu(root, 12), "Ready for review…");
    await settle();
    const reads = core.calls.filter((c) => c.cmd === "panel_prs").length;

    vi.advanceTimersByTime(POLL_MS);
    await settle();
    await settle();

    expect(
      core.calls.filter((c) => c.cmd === "panel_prs").length,
    ).toBeGreaterThan(reads);
    expect(prRow(root, 12).textContent).toContain(
      "Mark #12 ready for review on GitHub?",
    );
  } finally {
    vi.useRealTimers();
  }
});

test("a main folder that cannot be read leaves the other repos and their PRs", async () => {
  const core = prsCore({}, true);
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  const text = changesCard(root).textContent;
  expect(text).toMatch(/alpha: .*folder not found/);
  expect(text).toContain("plans");
  expect(prTitles(root)).toEqual(["Change 4"]);
});

test("the ⋯ button closes its own menu, and another row's opens in its place", async () => {
  const core = prsCore();
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");
  // A real press sends pointerdown before click, which the outside-click check sees.
  const press = (number: number) => {
    const trigger = button(prRow(root, number), `More for #${String(number)}`);
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    trigger.click();
  };

  press(12);
  await settle();
  expect(prRow(root, 12).querySelector(".pr-menu")).not.toBeNull();
  press(12);
  await settle();
  expect(prRow(root, 12).querySelector(".pr-menu")).toBeNull();

  press(12);
  await settle();
  press(11);
  await settle();
  expect(prRow(root, 12).querySelector(".pr-menu")).toBeNull();
  expect(prRow(root, 11).querySelector(".pr-menu")).not.toBeNull();
});
