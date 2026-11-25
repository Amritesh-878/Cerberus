import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import {
  cleanup,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";
import {
  type AccountTokens,
  costText,
  countText,
  type ModelRow,
  tokensText,
  totalCost,
} from "./tokens";

afterEach(cleanup);

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: String.raw`C:\code\alpha` }],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: null,
      account: 1,
    },
  ],
  nextId: 4,
};

const OPEN_PANEL: Profiles = {
  ...SAVED,
  panel: { ...SAVED.panel, open: true },
};

function model(name: string, input: number, cost: number | null): ModelRow {
  return {
    model: name,
    tokens: {
      input,
      output: 34_000,
      cacheRead: 8_100_000,
      cacheWrite: 450_000,
    },
    cost,
  };
}

const COUNTS: AccountTokens[] = [
  {
    account: 1,
    today: [
      model("Opus 5.5", 1_200_000, 3.1),
      model("claude-opus-4-5", 5, null),
    ],
    week: [model("Opus 5.5", 9_000_000, 22.4)],
    month: [model("Opus 5.5", 30_000_000, 80)],
  },
  { account: 0, today: [], week: [], month: [model("Haiku 5.5", 900, 0.004)] },
];

function tokensCard(root: HTMLElement): HTMLElement {
  const found = root.querySelector<HTMLElement>(".token-usage");
  if (!found) throw new Error("no tokens section");
  return found;
}

test("tokens and their numbers read plainly", () => {
  expect(countText(950)).toBe("950");
  expect(countText(34_400)).toBe("34k");
  expect(countText(1_200_000)).toBe("1.2M");
  expect(countText(3_000_000)).toBe("3M");
  expect(costText(3.104)).toBe("$3.10");
  expect(costText(0.004)).toBe("<$0.01");
  expect(costText(0)).toBe("$0.00");
  expect(costText(null)).toBe("no price");
  expect(tokensText(model("x", 1_200_000, null).tokens)).toBe(
    "1.2M in · 34k out · 8.1M cache read · 450k cache write",
  );
  expect(totalCost([model("a", 1, 1.5), model("b", 1, null)])).toBe(1.5);
});

test("the Usage card lists tokens per account and model for the picked range, priced or not", async () => {
  const pty = ptyCore();
  const asked: unknown[] = [];
  const root = await renderApp(() => OPEN_PANEL, {
    ...pty.core,
    tree_load: () => ({ tree: TREE, notice: null }),
    panel_changes: () => ({ kind: "notRepo" }),
    panel_prs: () => [],
    panel_connectors: () => ({ servers: [], unreadable: [] }),
    panel_skills: () => [],
    usage_tokens: (args: unknown) => {
      asked.push(field(args, "offsetMinutes"));
      return COUNTS;
    },
  });
  await open(root, "planner");
  await settle();

  const section = tokensCard(root);
  expect(asked[0]).toBe(-new Date().getTimezoneOffset());
  const rows = () =>
    [...section.querySelectorAll(".token-row")].map((r) => r.textContent);
  expect(rows()).toEqual([
    "Opus 5.5$3.101.2M in · 34k out · 8.1M cache read · 450k cache write",
    "claude-opus-4-5no price5 in · 34k out · 8.1M cache read · 450k cache write",
  ]);
  expect(section.querySelector(".token-account-head")?.textContent).toBe(
    "work$3.10",
  );
  expect(section.textContent).toContain("API price estimate, not your bill");

  const month = [...section.querySelectorAll("button")].find(
    (b) => b.textContent === "30 days",
  );
  month?.click();
  await settle();
  expect(month?.getAttribute("aria-pressed")).toBe("true");
  expect(section.querySelectorAll(".token-account")).toHaveLength(2);
  expect(rows()[1]).toMatch(/^Haiku 5\.5<\$0\.01900 in/);
});
