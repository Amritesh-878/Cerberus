import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Profiles } from "./accounts";
import { resetText } from "./board";
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
import {
  elapsedText,
  currentUse,
  type LiveView,
  meterLevel,
  resetWhen,
  type Segment,
  segmentText,
  untilText,
  warningText,
} from "./live";
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
const APP_PATH = String.raw`C:\code\alpha\src\app.ts`;
const README_PATH = String.raw`C:\code\alpha\readme.md`;
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

function segment(account: number, set: Partial<Segment>): Segment {
  return {
    account,
    used: null,
    resetsAt: null,
    asOf: null,
    lastReading: null,
    silent: false,
    ...set,
  };
}

const VIEW: LiveView = {
  tabs: [
    {
      tab: 3,
      account: 1,
      context: 42.5,
      contextTokens: 85_000,
      cacheExpiresAt: null,
      model: "Opus 5.5",
      keptTheirs: false,
      outputs: [],
      tasks: [],
      done: [],
      finished: 0,
    },
  ],
  meters: [
    {
      account: 1,
      fiveHour: { used: 87.2, resetsAt: RESET },
      sevenDay: { used: 41, resetsAt: RESET + 86_400 },
    },
  ],
  pool: {
    fiveHour: [segment(1, { used: 87.2, resetsAt: RESET }), segment(0, {})],
    sevenDay: [
      segment(1, { used: 41, resetsAt: RESET + 86_400 }),
      segment(0, {}),
    ],
  },
  warnings: [],
};

test("the context card shows the shown chat's use, and Compact types /compact without Enter", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");

  expect(card(root, "Context").textContent).toContain(
    "Restart the chat to show context",
  );
  await emitEvent("live", {
    ...VIEW,
    tabs: [{ ...VIEW.tabs[0], context: null, model: null }],
  });
  await settle();
  expect(card(root, "Context").textContent).toContain(
    "Shows after the chat's first reply",
  );
  await emitEvent("live", {
    ...VIEW,
    tabs: [{ ...VIEW.tabs[0], context: 0 }],
  });
  await settle();
  expect(card(root, "Context").querySelector(".card-meta")?.textContent).toBe(
    "0%",
  );
  expect(card(root, "Context").querySelector(".meter")).not.toBeNull();
  await emitEvent("live", VIEW);
  await settle();
  const context = card(root, "Context");
  click(context, "Compact");
  await settle();

  expect(context.querySelector(".card-meta")?.textContent).toBe("43%");
  expect(context.querySelector(".meter")?.getAttribute("aria-valuenow")).toBe(
    "43",
  );
  expect(context.querySelector(".meter")?.getAttribute("title")).toBe(
    "Opus 5.5",
  );
  // One row: the title, the meter, the percentage and Compact, with nothing to fold.
  expect(context.textContent).toBe("Context43%Compact");
  expect(context.querySelector(".card-body")).toBeNull();
  expect(context.querySelector("[aria-expanded]")).toBeNull();
  expect(context.querySelector(".card-compact")).not.toBeNull();
  expect(live.writes).toEqual(["\x1b[200~/compact\x1b[201~"]);
});

function bars(root: HTMLElement): HTMLElement[][] {
  return [...card(root, "Usage").querySelectorAll(".pool-bar")].map((bar) => [
    ...bar.querySelectorAll<HTMLElement>(".pool-seg"),
  ]);
}

const detail = (root: HTMLElement) =>
  card(root, "Usage").querySelector(".pool-detail")?.textContent;

test("each window is one bar with a segment per account, and a tap says its account, use and reset", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await emitEvent("live", VIEW);
  await settle();

  const [five, seven] = bars(root);
  expect(five).toHaveLength(2);
  expect(seven).toHaveLength(2);
  // 87% is past the 85% warning, so its 5-hour segment turns amber; the 7-day keeps the account's colour.
  expect(five?.[0]?.classList.contains("warn")).toBe(true);
  expect(seven?.[0]?.classList.contains("warn")).toBe(false);
  expect(
    seven?.[0]?.querySelector<HTMLElement>(".pool-fill")?.style.background,
  ).not.toBe("");
  expect(five?.[1]?.classList.contains("unread")).toBe(true);
  expect(five?.[1]?.getAttribute("title")).toBe("personal: no reading yet");

  five?.[0]?.click();
  await settle();
  expect(detail(root)).toMatch(
    new RegExp(
      `^5-hour · work: 87% · resets ${resetWhen(RESET, Math.floor(Date.now() / 1000))}, in \\d+d \\d+h$`,
    ),
  );
  expect(five?.[0]?.getAttribute("aria-pressed")).toBe("true");
  five?.[0]?.click();
  await settle();
  expect(detail(root)).toBe("Tap a segment for its account, use and reset.");
});

test("a silent plan, a reading kept from before a restart and a window with no reading since each say so", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  const keptAt = Date.now() - 600_000;
  await emitEvent("live", {
    ...VIEW,
    pool: {
      fiveHour: [
        segment(1, { used: 30, resetsAt: RESET, asOf: keptAt }),
        segment(0, { silent: true }),
      ],
      sevenDay: [segment(1, { lastReading: keptAt }), segment(0, {})],
    },
  });
  await settle();

  const [five, seven] = bars(root);
  const at = resetText(Math.floor(keptAt / 1000));
  expect(five?.[0]?.classList.contains("kept")).toBe(true);
  expect(five?.[0]?.getAttribute("title")).toMatch(
    new RegExp(`· as of ${at}$`),
  );
  expect(five?.[1]?.getAttribute("title")).toBe(
    "personal: this plan doesn't report usage",
  );
  seven?.[0]?.click();
  await settle();
  expect(detail(root)).toBe(`7-day · work: no reading since ${at}`);
  expect(
    card(root, "Usage").querySelector(".card-title")?.getAttribute("title"),
  ).toContain("an account updates after a reply in one of its tabs");
});

test("a reading past its reset reads as no reading since it came, in the text and the bar", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  const readAt = Date.now() - 3_600_000;
  const nowS = Math.floor(Date.now() / 1000);
  const past = segment(1, {
    used: 70,
    resetsAt: nowS - 60,
    lastReading: readAt,
  });
  await emitEvent("live", {
    ...VIEW,
    pool: { fiveHour: [past, segment(0, {})], sevenDay: VIEW.pool.sevenDay },
  });
  await settle();

  const [five] = bars(root);
  expect(five?.[0]?.classList.contains("unread")).toBe(true);
  expect(five?.[0]?.querySelector<HTMLElement>(".pool-fill")?.style.width).toBe(
    "0%",
  );
  expect(five?.[0]?.getAttribute("title")).toBe(
    `work: no reading since ${resetText(Math.floor(readAt / 1000))}`,
  );
  expect(currentUse(past, nowS)).toBeNull();
});

test("segmentText reads plainly", () => {
  expect(
    segmentText("work", segment(1, { used: 12.4, resetsAt: 3600 }), 0),
  ).toBe(`work: 12% · resets ${resetText(3600)}, in 1h 0m`);
  expect(segmentText("work", segment(1, {}), 0)).toBe("work: no reading yet");
  const weekAway = 3 * 86_400;
  expect(resetWhen(weekAway, 0, "en-US")).toBe(
    `${new Date(weekAway * 1000).toLocaleDateString("en-US", { weekday: "short" })} ${resetText(weekAway, "en-US")}`,
  );
  expect(resetWhen(3600, 0, "en-US")).toBe(resetText(3600, "en-US"));
});

const WARNING = {
  account: 1,
  tab: 3,
  used: 87.2,
  resetsAt: RESET,
  offers: [0],
};

/** The core's view holding `warning`, then its once-per-window event, as the core sends them. */
async function warn(warning: typeof WARNING): Promise<void> {
  await emitEvent("live", { ...VIEW, warnings: [warning] });
  await emitEvent("live-warning", warning);
  await settle();
}

function popup(): HTMLElement | null {
  return document.querySelector<HTMLElement>(".limit-popup");
}

test("the warning is a popup over its tab, and its account button moves the chat in one click", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");

  await warn(WARNING);

  expect(popup()?.querySelector("h2")?.textContent).toMatch(
    /^work is at 87%, resets /,
  );
  expect(popup()?.textContent).toContain("Move planner to:");
  expect(popup()?.textContent).toContain("Stops anything it started.");
  expect(desktop.sent, "its tab is on screen").toEqual([]);
  // The first account below the line is the default.
  expect(document.activeElement?.getAttribute("aria-label")).toBe(
    "Move to personal",
  );
  click(popup() ?? root, "Move to personal");
  await settle();

  expect(root.querySelector(".menu-confirm")).toBeNull();
  expect(popup()).toBeNull();
  expect(live.calls.find((c) => c.cmd === "continue_on_account")?.args).toEqual(
    { tab: 3, account: 0 },
  );
  expect(live.calls.find((c) => c.cmd === "live_dismiss")?.args).toEqual({
    account: 1,
  });
});

test("Not now leaves a bar whose Show opens the popup again, and Dismiss tells the core", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await warn(WARNING);

  click(popup() ?? root, "Not now");
  await settle();
  expect(popup()).toBeNull();
  const bar = root.querySelector<HTMLElement>(".live-warning");
  expect(bar?.textContent).toContain("work is at 87%, resets ");
  expect(() => button(bar ?? root, "Move to personal")).toThrow();
  click(bar ?? root, "Show");
  await settle();
  expect(popup()?.textContent).toContain("Move planner to:");
  expect(root.querySelector(".live-warning")).toBeNull();

  click(popup() ?? root, "Not now");
  await settle();
  click(root.querySelector(".live-warning") ?? root, "Dismiss");
  await settle();
  expect(live.calls.find((c) => c.cmd === "live_dismiss")?.args).toEqual({
    account: 1,
  });
  expect(live.calls.map((c) => c.cmd)).not.toContain("continue_on_account");
});

test("a warning with no signed-in account below the line offers no move", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await warn({ ...WARNING, used: 90, offers: [] });

  expect(popup()?.textContent).toContain(
    "No other signed-in account is under 85%",
  );
  expect(() => button(popup() ?? root, "Move to personal")).toThrow();
});

test("a warning while the window is behind another waits for the focus, with a desktop notification", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await emitEvent("tauri://blur");

  await warn(WARNING);

  expect(popup()).toBeNull();
  expect(desktop.sent).toHaveLength(1);
  await emitEvent("tauri://focus");
  await settle();
  expect(popup()?.textContent).toContain("Move planner to:");
});

test("a warning for an account its tab no longer uses opens nothing and offers no Show", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");

  // The planner is on work (1); the warning is about personal (0).
  await warn({ ...WARNING, account: 0, offers: [1] });

  expect(popup()).toBeNull();
  const bar = root.querySelector<HTMLElement>(".live-warning");
  expect(bar?.textContent).toContain("personal is at 87%");
  expect(() => button(bar ?? root, "Show")).toThrow();
});

test("a full window's bar offers no Show, since its popup is the limit popup", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  await warn({ ...WARNING, used: 100 });

  click(popup() ?? root, "Not now");
  await settle();

  expect(popup()).toBeNull();
  const bar = root.querySelector<HTMLElement>(".live-warning");
  expect(bar?.textContent).toContain("work is at 100%");
  expect(() => button(bar ?? root, "Show")).toThrow();
});

test("a warning for a tab not on screen waits for it, with a desktop notification meanwhile", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);

  await warn(WARNING);

  expect(popup()).toBeNull();
  expect(desktop.sent).toHaveLength(1);
  await open(root, "planner");
  expect(popup()?.textContent).toContain("Move planner to:");
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

test("the outputs card lists written files, and its menu opens one through the core", async () => {
  const live = liveCore();
  const opened: unknown[] = [];
  const root = await renderApp(live.profiles, {
    ...live.core,
    live_open_output: (args: unknown) => {
      opened.push(args);
      return null;
    },
  });
  await open(root, "planner");
  await emitEvent("live", {
    ...VIEW,
    tabs: [
      {
        ...VIEW.tabs[0],
        outputs: [
          {
            name: "app.ts",
            folder: "src",
            path: APP_PATH,
            at: 2,
          },
          {
            name: "readme.md",
            folder: "",
            path: README_PATH,
            at: 1,
          },
        ],
      },
    ],
  });
  await settle();

  const rows = [...card(root, "Outputs").querySelectorAll(".card-row")];
  expect(rows.map((r) => r.textContent)).toEqual(["app.tssrc", "readme.md"]);
  // One action needs no menu: the row's own button opens the file in VS Code.
  expect(
    card(root, "Outputs").querySelector('[aria-haspopup="menu"]'),
  ).toBeNull();
  click(card(root, "Outputs"), "Open app.ts in VS Code");
  await settle();
  expect(opened).toEqual([{ tab: 3, path: APP_PATH }]);
});

test("the tasks card lists running then finished tasks by description, and a click types /tasks", async () => {
  const live = liveCore();
  const root = await renderApp(live.profiles, live.core);
  await open(root, "planner");
  const since = Date.now() - 75_000;
  await emitEvent("live", {
    ...VIEW,
    tabs: [
      {
        ...VIEW.tabs[0],
        tasks: [
          {
            id: "b1",
            kind: "shell",
            label: "shell",
            description: "Run the dev server",
            since,
            ended: null,
          },
          {
            id: "a1",
            kind: "subagent",
            label: "general-purpose",
            description: null,
            since,
            ended: null,
          },
        ],
        done: [
          {
            id: "b0",
            kind: "shell",
            label: "shell",
            description: "Build the app",
            since: 1_000,
            ended: 63_000,
          },
        ],
        finished: 3,
      },
    ],
  });
  await settle();

  const tasks = card(root, "Background tasks");
  expect(tasks.querySelector(".card-meta")?.textContent).toBe("2 running");
  const rows = [...tasks.querySelectorAll(".task-row")].map(
    (r) => r.textContent,
  );
  expect(rows[0]).toMatch(
    /^general-purposesubagent · general-purpose · running1m 1[4-6]s$/,
  );
  expect(rows[1]).toMatch(/^Run the dev servershell · running1m 1[4-6]s$/);
  expect(rows[2]).toBe("Build the appshell · done1m 02s");
  expect(tasks.textContent).toContain("3 finished in all");

  click(tasks, "Build the appshell · done1m 02s");
  await settle();
  expect(live.writes).toEqual(["\x1b[200~/tasks\x1b[201~"]);
});

test("the warning text, elapsed time and time to a reset", () => {
  expect(elapsedText(45_000)).toBe("45s");
  expect(elapsedText(192_000)).toBe("3m 12s");
  expect(elapsedText(3_840_000)).toBe("1h 04m");

  expect(warningText("work", { used: 86.6, resetsAt: 0 }, "en-US")).toMatch(
    /^work is at 87%, resets \d{1,2}:\d{2} (am|pm)$/,
  );
  expect(untilText(10_000 + 2 * 3600 + 10 * 60, 10_000)).toBe("in 2h 10m");
  expect(untilText(10_000 + 45 * 60, 10_000)).toBe("in 45m");
  expect(untilText(10_000 + 3 * 86_400 + 4 * 3600, 10_000)).toBe("in 3d 4h");
  expect(untilText(10_000, 10_000)).toBe("in under a minute");
  expect(meterLevel(84.9, 85)).toBe("ok");
  expect(meterLevel(85, 85)).toBe("warn");
  expect(meterLevel(100, 85)).toBe("full");
});
