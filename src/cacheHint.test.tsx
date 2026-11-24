import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { BIG_CONTEXT, CACHE_IDLE_MS, cacheHint } from "./cacheHint";
import {
  click,
  cleanup,
  emit,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { LiveView } from "./live";
import type { SavedTree } from "./saved";

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(async () => {
  vi.useRealTimers();
  await cleanup();
});

const HOUR = CACHE_IDLE_MS;

test("the hint needs an hour idle and a big chat", () => {
  expect(cacheHint(0, 412_400, HOUR)).toBe(
    "Idle for over an hour: the next reply rereads 412k tokens.",
  );
  expect(cacheHint(0, 412_400, HOUR - 1)).toBeUndefined();
  expect(cacheHint(0, BIG_CONTEXT - 1, HOUR)).toBeUndefined();
  expect(cacheHint(undefined, 412_400, HOUR)).toBeUndefined();
  expect(cacheHint(0, null, HOUR)).toBeUndefined();
});

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

function view(tokens: number): LiveView {
  return {
    tabs: [
      {
        tab: 3,
        account: 1,
        context: 41,
        contextTokens: tokens,
        model: "Opus 5.5",
        keptTheirs: false,
        outputs: [],
        tasks: [],
        done: [],
        finished: 0,
      },
    ],
    meters: [],
    warnings: [],
  };
}

async function idleChat(tokens: number) {
  const pty = ptyCore();
  const writes: string[] = [];
  const root = await renderApp(() => SAVED, {
    ...pty.core,
    tree_load: () => ({ tree: TREE, notice: null }),
    live_view: () => view(tokens),
    pty_write: (args: unknown) => {
      writes.push(String(field(args, "data")));
    },
  });
  await open(root, "planner");
  emit(pty.spawned, 0, { state: "idle" });
  await emitEvent("live", view(tokens));
  await settle();
  return { root, writes, pty };
}

const hint = (root: HTMLElement) =>
  root.querySelector(".cache-hint span")?.textContent;

test("after an hour idle a big chat says so, and Type /compact types it without Enter", async () => {
  const { root, writes } = await idleChat(412_000);
  expect(hint(root)).toBeUndefined();

  await vi.advanceTimersByTimeAsync(HOUR + 60_000);
  expect(hint(root)).toBe(
    "Idle for over an hour: the next reply rereads 412k tokens.",
  );

  click(root, "Type /compact");
  await settle();
  expect(writes).toEqual(["\x1b[200~/compact\x1b[201~"]);
  expect(hint(root)).toBeUndefined();
});

test("the hint goes once he types, and a turn starts the clock over", async () => {
  const { root, pty } = await idleChat(412_000);
  await vi.advanceTimersByTimeAsync(HOUR + 60_000);
  expect(hint(root)).toBeDefined();

  const area = root.querySelector<HTMLTextAreaElement>(
    ".xterm-helper-textarea",
  );
  area?.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "h",
      keyCode: 72,
      bubbles: true,
      cancelable: true,
    }),
  );
  await settle();
  expect(hint(root)).toBeUndefined();

  emit(pty.spawned, 0, { state: "working" }, { state: "idle" });
  await vi.advanceTimersByTimeAsync(HOUR - 60_000);
  expect(hint(root), "idle again for less than an hour").toBeUndefined();
});

test("a small chat never shows the hint", async () => {
  const { root } = await idleChat(150_000);

  await vi.advanceTimersByTimeAsync(2 * HOUR);

  expect(hint(root)).toBeUndefined();
});
