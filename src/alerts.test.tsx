import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Profiles } from "./accounts";
import { resetText } from "./board";
import {
  activeTitle,
  cleanup,
  click,
  emit,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";

// Permission and sending go through the webview's Notification shim, which only the real app has.
const desktop = vi.hoisted(() => ({
  granted: true,
  // Set to hold the permission answer back until the test releases it.
  answer: undefined as Promise<boolean> | undefined,
  asked: 0,
  sent: [] as unknown[],
}));
vi.mock("@tauri-apps/plugin-notification", async (original) => ({
  ...(await original<typeof import("@tauri-apps/plugin-notification")>()),
  isPermissionGranted: () => desktop.answer ?? Promise.resolve(desktop.granted),
  requestPermission: () => {
    desktop.asked += 1;
    return Promise.resolve(desktop.granted ? "granted" : "denied");
  },
  sendNotification: (options: unknown) => {
    desktop.sent.push(options);
  },
}));

beforeEach(() => {
  desktop.granted = true;
  desktop.answer = undefined;
  desktop.asked = 0;
  desktop.sent = [];
});
afterEach(cleanup);

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: String.raw`C:\code\alpha` }],
  tabs: [
    { id: 3, projectId: 1, parentId: null, title: "planner" },
    { id: 4, projectId: 1, parentId: 3, title: "coder" },
  ].map((tab) => ({
    ...tab,
    program: "claude",
    cwd: String.raw`C:\code\alpha`,
    account: 1,
  })),
  nextId: 5,
};

const WORKING = { state: "working" };
const IDLE = { state: "idle" };

function app(profiles: Profiles = SAVED) {
  const pty = ptyCore();
  const clicks: { id: number }[] = [];
  const windowCalls: string[] = [];
  const render = () =>
    renderApp(() => profiles, {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      "plugin:notification|register_listener": (args: unknown) => {
        clicks.push(field(args, "handler") as { id: number });
      },
      "plugin:window|unminimize": () => {
        windowCalls.push("unminimize");
      },
      "plugin:window|set_focus": () => {
        windowCalls.push("set_focus");
      },
    });
  return { pty, clicks, windowCalls, render };
}

// Clicks the notification as the plugin reports it: the payload carries what was sent.
function clickNotification(clicks: { id: number }[], sent: unknown): void {
  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void };
    }
  ).__TAURI_INTERNALS__;
  for (const channel of clicks) {
    internals.runCallback(channel.id, {
      index: 0,
      message: { actionId: "tap", inputValue: null, notification: sent },
    });
  }
}

test("a finished turn in a hidden tab shows one notification, and clicking it opens that tab", async () => {
  const { pty, clicks, windowCalls, render } = app();
  const root = await render();
  await open(root, "planner");
  await open(root, "coder");

  emit(pty.spawned, 0, WORKING, IDLE, IDLE);
  await settle();

  expect(desktop.sent).toEqual([
    { title: "planner · alpha", body: "Waiting on you", extra: { tab: 3 } },
  ]);
  expect(activeTitle(root)).toBe("coder");

  clickNotification(clicks, { id: 1, ...(desktop.sent[0] as object) });
  await settle();

  expect(activeTitle(root)).toBe("planner");
  expect(windowCalls).toEqual(["unminimize", "set_focus"]);
});

test("the shown tab alerts only once the window loses focus", async () => {
  const { pty, render } = app();
  const root = await render();
  await open(root, "planner");

  emit(pty.spawned, 0, WORKING, IDLE);
  await settle();
  expect(desktop.sent).toEqual([]);

  await emitEvent("tauri://blur");
  emit(pty.spawned, 0, WORKING, IDLE);
  await settle();
  expect(desktop.sent).toHaveLength(1);

  await emitEvent("tauri://focus");
  emit(pty.spawned, 0, WORKING, IDLE);
  await settle();
  expect(desktop.sent).toHaveLength(1);
});

test("a usage limit shows one notification with its reset time", async () => {
  const { pty, render } = app();
  const root = await render();
  await open(root, "planner");
  await open(root, "coder");
  const resetsAt = new Date(2026, 10, 2, 18, 40).getTime() / 1000;

  emit(pty.spawned, 0, WORKING, { state: "limited", resetsAt });
  emit(pty.spawned, 0, { state: "limited", resetsAt });
  await settle();

  expect(desktop.sent).toEqual([
    {
      title: "planner · alpha",
      body: `Hit its limit, resets ${resetText(resetsAt)}`,
      extra: { tab: 3 },
    },
  ]);
});

test("a chat that ends while working shows a notification", async () => {
  const { pty, render } = app();
  const root = await render();
  await open(root, "planner");
  await open(root, "coder");

  emit(pty.spawned, 0, WORKING, { exitCode: 0, ended: true });
  await settle();

  expect(desktop.sent).toEqual([
    { title: "planner · alpha", body: "The chat ended", extra: { tab: 3 } },
  ]);
});

test("alerts turned off show nothing", async () => {
  const { pty, render } = app({ ...SAVED, alerts: false });
  const root = await render();
  await open(root, "planner");
  await open(root, "coder");

  emit(pty.spawned, 0, WORKING, IDLE);
  await settle();

  expect(desktop.sent).toEqual([]);
});

test("a refused permission is asked for once and then stays quiet", async () => {
  desktop.granted = false;
  const { pty, render } = app();
  const root = await render();
  await open(root, "planner");
  await open(root, "coder");

  emit(pty.spawned, 0, WORKING, IDLE, WORKING, IDLE);
  await settle();

  expect(desktop.asked).toBe(1);
  expect(desktop.sent).toEqual([]);
});

test("an alert still waiting on the permission answer is dropped once you open its tab", async () => {
  let answer: (granted: boolean) => void = () => undefined;
  desktop.answer = new Promise((resolve) => {
    answer = resolve;
  });
  const { pty, render } = app();
  const root = await render();
  await open(root, "planner");
  await open(root, "coder");

  emit(pty.spawned, 0, WORKING, IDLE);
  await settle();
  await open(root, "planner");
  answer(true);
  await settle();

  expect(desktop.sent).toEqual([]);
});

test("a click for a tab that was closed opens nothing", async () => {
  const { clicks, windowCalls, render } = app();
  const root = await render();
  await open(root, "coder");

  clickNotification(clicks, { id: 1, title: "x", extra: { tab: 99 } });
  await settle();

  expect(activeTitle(root)).toBe("coder");
  expect(windowCalls).toEqual([]);
});

test("two quick clicks on the alerts box are saved in order", async () => {
  const saves: unknown[] = [];
  const pending: (() => void)[] = [];
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    set_alerts: (args: unknown) =>
      new Promise<Profiles>((resolve) => {
        saves.push(field(args, "on"));
        pending.push(() => {
          saved = {
            ...saved,
            alerts: field(args, "on") === true,
            revision: saves.length,
          };
          resolve(saved);
        });
      }),
  });
  click(root, "Accounts");
  await settle();
  const box = root.querySelector<HTMLInputElement>(".alerts-setting input");
  if (!box) throw new Error("no alerts box");

  box.click();
  box.click();
  await settle();
  expect(saves).toEqual([false]);

  pending.shift()?.();
  await settle();
  expect(saves).toEqual([false, true]);
  expect(box.checked).toBe(true);
  pending.shift()?.();
  await settle();

  expect(saved.alerts).toBe(true);
  expect(box.checked).toBe(true);
});

test("alerts saves stay in click order after Accounts is closed and reopened", async () => {
  const saves: unknown[] = [];
  const pending: (() => void)[] = [];
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    set_alerts: (args: unknown) =>
      new Promise<Profiles>((resolve) => {
        saves.push(field(args, "on"));
        pending.push(() => {
          saved = {
            ...saved,
            alerts: field(args, "on") === true,
            revision: saves.length,
          };
          resolve(saved);
        });
      }),
  });
  const box = async () => {
    click(root, "Accounts");
    await settle();
    const found = root.querySelector<HTMLInputElement>(".alerts-setting input");
    if (!found) throw new Error("no alerts box");
    return found;
  };

  const first = await box();
  first.click();
  first.click();
  click(root, "Done");
  await settle();
  (await box()).click();
  await settle();
  expect(saves).toEqual([false]);

  while (pending.length > 0) {
    pending.shift()?.();
    await settle();
  }

  expect(saves).toEqual([false, true, false]);
  expect(saved.alerts).toBe(false);
});

test("a choice still saving shows on a reopened Accounts, and its failure shows after closing", async () => {
  let finish: (() => void) | undefined;
  const root = await renderApp(() => SAVED, {
    set_alerts: () =>
      new Promise<Profiles>((_, reject) => {
        finish = () => {
          reject(new Error("disk full"));
        };
      }),
  });
  const box = async () => {
    click(root, "Accounts");
    await settle();
    const found = root.querySelector<HTMLInputElement>(".alerts-setting input");
    if (!found) throw new Error("no alerts box");
    return found;
  };

  (await box()).click();
  click(root, "Done");
  await settle();
  expect((await box()).checked).toBe(false);

  click(root, "Done");
  finish?.();
  await settle();
  expect(root.querySelector(".save-error")?.textContent).toBe(
    "Alerts setting not saved",
  );
  expect((await box()).checked).toBe(true);
});

test("the accounts screen turns alerts off and keeps the box in step on a failed save", async () => {
  let fail = false;
  const saves: unknown[] = [];
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    set_alerts: (args: unknown) => {
      saves.push(field(args, "on"));
      if (fail) throw new Error("disk full");
      saved = { ...saved, alerts: field(args, "on") === true };
      return saved;
    },
  });
  click(root, "Accounts");
  await settle();
  const box = root.querySelector<HTMLInputElement>(".alerts-setting input");
  if (!box) throw new Error("no alerts box");
  expect(box.checked).toBe(true);

  box.click();
  await settle();
  expect(saves).toEqual([false]);
  expect(box.checked).toBe(false);

  fail = true;
  box.click();
  await settle();
  expect(saves).toEqual([false, true]);
  expect(box.checked).toBe(false);
  expect(root.querySelector(".save-error")?.textContent).toBe(
    "Alerts setting not saved",
  );
  expect(root.querySelector(".save-error")?.getAttribute("title")).toContain(
    "disk full",
  );
});
