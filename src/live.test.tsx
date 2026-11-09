import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Profiles } from "./accounts";
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
import { current, type LiveView, warningText } from "./live";
import type { SavedTree } from "./saved";

const desktop = vi.hoisted(() => ({ sent: [] as unknown[] }));
vi.mock("@tauri-apps/plugin-notification", async (original) => ({
  ...(await original<typeof import("@tauri-apps/plugin-notification")>()),
  isPermissionGranted: () => Promise.resolve(true),
  sendNotification: (options: unknown) => {
    desktop.sent.push(options);
  },
}));

beforeEach(() => {
  desktop.sent = [];
});
afterEach(cleanup);

const ALPHA = String.raw`C:\code\alpha`;
// Far ahead, so no window has reset by the time a test reads it.
const RESET = 4_102_444_800;

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
    },
  ],
  nextId: 4,
};

const OPEN_PANEL: Profiles = {
  ...SAVED,
  panel: { ...SAVED.panel, open: true },
};

function liveCore(profiles: Profiles = OPEN_PANEL) {
  const pty = ptyCore();
  const writes: string[] = [];
  const calls: { cmd: string; args: unknown }[] = [];
  const record =
    (cmd: string, answer: unknown = null) =>
    (args: unknown) => {
      calls.push({ cmd, args });
      return answer;
    };
  return {
    writes,
    calls,
    profiles: () => profiles,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      panel_changes: () => ({ kind: "notRepo" }),
      panel_prs: () => [],
      panel_connectors: () => ({ servers: [], unreadable: [] }),
      panel_skills: () => [],
      pty_write: (args: unknown) => {
        writes.push(String(field(args, "data")));
      },
      live_dismiss: record("live_dismiss"),
      continue_on_account: record("continue_on_account"),
    },
  };
}

function card(root: HTMLElement, title: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".card")].find(
    (c) => c.querySelector(".card-title")?.textContent === title,
  );
  if (!found) throw new Error(`no ${title} card`);
  return found;
}

const VIEW: LiveView = {
  tabs: [
    {
      tab: 3,
      account: 1,
      context: 42.5,
      model: "Opus 5.5",
      keptTheirs: false,
    },
  ],
  meters: [
    {
      account: 1,
      fiveHour: { used: 87.2, resetsAt: RESET },
      sevenDay: { used: 41, resetsAt: RESET + 86_400 },
    },
  ],
  warnings: [],
};

test("the context card shows the shown chat's use, and Compact types /compact without Enter", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");

  expect(card(root, "Context").textContent).toContain(
    "Restart the chat to see its context here",
  );
  await emitEvent("live", {
    ...VIEW,
    tabs: [{ ...VIEW.tabs[0], context: null, model: null }],
  });
  await settle();
  expect(card(root, "Context").textContent).toContain(
    "Shows after the chat's first reply",
  );
  await emitEvent("live", VIEW);
  await settle();
  const context = card(root, "Context");
  click(context, "Compact");
  await settle();

  expect(context.querySelector(".card-meta")?.textContent).toBe("43%");
  expect(context.querySelector(".meter")?.getAttribute("aria-valuenow")).toBe(
    "43",
  );
  expect(context.textContent).toContain("Opus 5.5");
  expect(live.writes).toEqual(["/compact"]);
});

test("usage has a row per account, unknown until one of its tabs reports", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await emitEvent("live", VIEW);
  await settle();

  const rows = [
    ...card(root, "Usage").querySelectorAll<HTMLElement>(".usage-account"),
  ].map((row) => row.textContent);

  expect(rows[0]).toBe("personal5-hourunknown7-dayunknown");
  expect(rows[1]).toMatch(/^work5-hour87% · resets .+7-day41% · resets .+$/);
  expect(
    card(root, "Usage")
      .querySelector(".usage-account + .usage-account .meter")
      ?.classList.contains("high"),
  ).toBe(true);
});

test("a warning offers Continue on, which only opens the usual confirm, and Dismiss tells the core", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await emitEvent("live", {
    ...VIEW,
    warnings: [
      { account: 1, tab: 3, used: 87.2, resetsAt: RESET, offers: [0] },
    ],
  });
  await settle();

  const banner = root.querySelector<HTMLElement>(".live-warning");
  expect(banner?.textContent).toContain("work is at 87%, resets ");
  click(banner ?? root, "Continue on personal");
  await settle();

  expect(root.querySelector(".menu-confirm")?.textContent).toContain(
    "Close this chat and continue it on personal?",
  );
  expect(live.calls.filter((c) => c.cmd === "continue_on_account")).toEqual([]);
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();
  expect(live.calls.find((c) => c.cmd === "continue_on_account")?.args).toEqual(
    { tab: 3, account: 0 },
  );

  click(root.querySelector(".live-warning") ?? root, "Dismiss");
  await settle();
  expect(live.calls.find((c) => c.cmd === "live_dismiss")?.args).toEqual({
    account: 1,
  });
});

test("a warning with no signed-in account below the line offers no move", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await emitEvent("live", {
    ...VIEW,
    warnings: [{ account: 1, tab: 3, used: 90, resetsAt: RESET, offers: [] }],
  });
  await settle();

  const banner = root.querySelector<HTMLElement>(".live-warning");
  expect(banner?.textContent).toContain(
    "No other signed-in account is below 85%",
  );
  expect(() => button(banner ?? root, "Continue on personal")).toThrow();
});

test("the core's warning event sends one desktop notification naming the account", async () => {
  const live = liveCore();
  await renderApp(live.profiles, live.core);

  await emitEvent("live-warning", {
    account: 1,
    tab: 3,
    used: 87.2,
    resetsAt: RESET,
    offers: [],
  });
  await settle();

  expect(desktop.sent).toHaveLength(1);
  expect(desktop.sent[0]).toMatchObject({
    title: "Usage limit",
    extra: { tab: 3 },
  });
  expect(String(field(desktop.sent[0], "body"))).toMatch(
    /^work is at 87%, resets /,
  );
});

test("the warning text and a window past its reset", () => {
  expect(warningText("work", { used: 86.6, resetsAt: 0 }, "en-US")).toMatch(
    /^work is at 87%, resets \d{1,2}:\d{2}(am|pm)$/,
  );
  expect(current({ used: 50, resetsAt: 100 }, 99)).not.toBeNull();
  expect(current({ used: 50, resetsAt: 100 }, 100)).toBeNull();
  expect(current(null, 0)).toBeNull();
});
