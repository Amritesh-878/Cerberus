import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";

import {
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

afterEach(cleanup);

const FOLDER = String.raw`C:\code\alpha`;
const RESET = 1_900_000_000;
const PASTED = "\x1b[200~continue\x1b[201~";

/** One project with a Claude tab on "work" (account 1); "personal" (0) is below the line. */
const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: FOLDER }],
  tabs: [
    {
      id: 2,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: FOLDER,
      account: 1,
    },
  ],
  nextId: 3,
};

function limitCore(offers: number[] = [0]) {
  const pty = ptyCore();
  const calls: { cmd: string; args: unknown }[] = [];
  const writes: string[] = [];
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  return {
    pty,
    calls,
    writes,
    moves: () =>
      calls.filter((c) => c.cmd === "continue_on_account").map((c) => c.args),
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      account_status: () => ({
        state: "loggedIn",
        summary: "user@example.com · Pro",
      }),
      pty_write: (args: unknown) => {
        writes.push(String(field(args, "data")));
      },
      ...record("live_offers", () => offers),
      ...record("continue_on_account", () => null),
    },
  };
}

function popup(): HTMLElement | null {
  return document.querySelector<HTMLElement>(".limit-popup");
}

async function hitLimit(
  env: ReturnType<typeof limitCore>,
  nth = 0,
): Promise<void> {
  emit(env.pty.spawned, nth, { state: "limited", resetsAt: RESET });
  await settle();
  await settle();
}

test("a chat hitting its limit gets one popup per window, offering only accounts below the line", async () => {
  const env = limitCore([0]);
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");

  await hitLimit(env);

  expect(
    field(env.calls.find((c) => c.cmd === "live_offers")?.args, "account"),
  ).toBe(1);
  expect(popup()?.textContent).toContain("planner hit its limit");
  expect(popup()?.textContent).toContain("work is limited until");
  const buttons = [...(popup()?.querySelectorAll("button") ?? [])].map((b) =>
    b.textContent.trim(),
  );
  expect(buttons).toEqual(["Continue on personal", "Not now"]);
  expect(
    [...(popup()?.querySelectorAll("button") ?? [])].every((b) =>
      b.classList.contains("secondary"),
    ),
  ).toBe(true);
  expect(env.moves()).toEqual([]);

  click(popup() ?? root, "Not now");
  await settle();
  await hitLimit(env);

  expect(popup()).toBeNull();
  expect(env.moves()).toEqual([]);
  expect(env.writes).toEqual([]);
});

test("Continue moves the chat, and continue is typed as a paste only once the new chat is idle, never sent", async () => {
  const env = limitCore([0]);
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");
  await hitLimit(env);

  click(popup() ?? root, "Continue on personal");
  await settle();
  await settle();

  expect(popup()).toBeNull();
  expect(env.moves()).toEqual([{ tab: 2, account: 0 }]);
  const restarted = env.pty.spawned.length - 1;
  expect(field(env.pty.spawned[restarted], "account")).toBe(0);
  expect(env.writes).toEqual([]);

  emit(env.pty.spawned, restarted, { state: "working" });
  await settle();
  expect(env.writes).toEqual([]);
  emit(env.pty.spawned, restarted, { state: "idle" });
  await settle();

  expect(env.writes).toEqual([PASTED]);
  expect(env.writes.join("")).not.toContain("\r");
});

test("no account below the line offers no move, only Not now", async () => {
  const env = limitCore([]);
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");

  await hitLimit(env);

  expect(popup()?.textContent).toContain(
    "No other signed-in account is below 85%",
  );
  const buttons = [...(popup()?.querySelectorAll("button") ?? [])].map((b) =>
    b.textContent.trim(),
  );
  expect(buttons).toEqual(["Not now"]);
});

test("a Continue on from the tab's menu types nothing after the move", async () => {
  const env = limitCore([0]);
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");

  root.querySelector<HTMLButtonElement>(".account-pill")?.click();
  await settle();
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();
  await settle();
  const restarted = env.pty.spawned.length - 1;
  emit(env.pty.spawned, restarted, { state: "idle" });
  await settle();

  expect(env.moves()).toEqual([{ tab: 2, account: 0 }]);
  expect(env.writes).toEqual([]);
});

test("a move from the low-limit banner types continue too, once its confirm is clicked", async () => {
  const env = limitCore([0]);
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");
  await emitEvent("live", {
    tabs: [],
    meters: [],
    warnings: [{ account: 1, tab: 2, used: 90, resetsAt: RESET, offers: [0] }],
  });
  await settle();

  click(root.querySelector(".live-warning") ?? root, "Continue on personal");
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();
  await settle();
  emit(env.pty.spawned, env.pty.spawned.length - 1, { state: "idle" });
  await settle();

  expect(env.moves()).toEqual([{ tab: 2, account: 0 }]);
  expect(env.writes).toEqual([PASTED]);
});

test("a meter's 5-hour window at 100% opens the popup for the tab that reported it, once", async () => {
  const env = limitCore([0]);
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");
  const full = {
    tabs: [],
    meters: [],
    warnings: [{ account: 1, tab: 2, used: 100, resetsAt: RESET, offers: [0] }],
  };

  await emitEvent("live", full);
  await settle();
  expect(popup()?.textContent).toContain("work is limited until");
  click(popup() ?? root, "Not now");
  await settle();
  await emitEvent("live", {
    ...full,
    warnings: [{ ...full.warnings[0], used: 100 }],
  });
  await settle();
  await hitLimit(env);

  expect(popup()).toBeNull();
  expect(env.moves()).toEqual([]);
});
