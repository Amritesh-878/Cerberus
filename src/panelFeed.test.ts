import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createRoot, createSignal } from "solid-js";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { field } from "./harness";
import { createPanelFeed, POLL_MS, type Target } from "./panelFeed";

let dispose: (() => void) | undefined;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  dispose?.();
  dispose = undefined;
  vi.useRealTimers();
  clearMocks();
});

const A: Target = { folder: String.raw`C:\a`, account: 1 };
const B: Target = { folder: String.raw`C:\b`, account: 2 };

/** A core that records each read and can hold `panel_changes` replies to release them in any order. */
function core(options: { hold?: boolean } = {}) {
  const calls: string[] = [];
  const held: (() => void)[] = [];
  mockIPC((cmd, args) => {
    const folder = String(field(args, "folder"));
    calls.push(`${cmd} ${folder}`);
    switch (cmd) {
      case "panel_changes": {
        const reply = {
          kind: "repo",
          name: folder,
          branch: "main",
          commit: null,
          base: null,
          added: calls.length,
          removed: 0,
          gh: true,
        };
        if (!options.hold) return reply;
        return new Promise((resolve) =>
          held.push(() => {
            resolve(reply);
          }),
        );
      }
      case "panel_pr":
        return null;
      case "panel_connectors":
        return { servers: [], unreadable: [] };
      case "panel_skills":
        return [];
      default:
        return null;
    }
  });
  return { calls, held };
}

function feedFor(initial: Target | undefined) {
  const [target, setTarget] = createSignal(initial);
  const feed = createRoot((stop) => {
    dispose = stop;
    return createPanelFeed(target);
  });
  return { feed, setTarget };
}

async function flush(): Promise<void> {
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(0);
}

test("a feed reads every card once on start, then only git on each poll", async () => {
  const { calls } = core();
  feedFor(A);
  await flush();

  expect(calls.sort()).toEqual([
    String.raw`panel_changes C:\a`,
    String.raw`panel_connectors C:\a`,
    String.raw`panel_pr C:\a`,
    String.raw`panel_skills C:\a`,
  ]);

  calls.length = 0;
  await vi.advanceTimersByTimeAsync(POLL_MS);

  expect(calls.sort()).toEqual([
    String.raw`panel_changes C:\a`,
    String.raw`panel_pr C:\a`,
  ]);
});

test("a tab change reads the new tab's cards and polls only the new folder", async () => {
  const { calls } = core();
  const { setTarget } = feedFor(A);
  await flush();

  calls.length = 0;
  setTarget(B);
  await flush();
  await vi.advanceTimersByTimeAsync(POLL_MS);

  expect(calls.every((c) => c.endsWith(String.raw`C:\b`))).toBe(true);
  expect(calls.filter((c) => c.startsWith("panel_changes"))).toHaveLength(2);
});

test("nothing is read once the feed is gone, as when the panel closes", async () => {
  const { calls } = core();
  feedFor(A);
  await flush();
  dispose?.();
  dispose = undefined;

  calls.length = 0;
  await vi.advanceTimersByTimeAsync(POLL_MS * 3);

  expect(calls).toEqual([]);
});

test("no tab on screen reads nothing", async () => {
  const { calls } = core();
  const { feed } = feedFor(undefined);
  await flush();
  await vi.advanceTimersByTimeAsync(POLL_MS);

  expect(calls).toEqual([]);
  expect(feed.changes()).toBeUndefined();
});

test("a hidden window skips the poll", async () => {
  const { calls } = core();
  feedFor(A);
  await flush();
  Object.defineProperty(document, "hidden", {
    value: true,
    configurable: true,
  });

  calls.length = 0;
  await vi.advanceTimersByTimeAsync(POLL_MS);
  Object.defineProperty(document, "hidden", {
    value: false,
    configurable: true,
  });

  expect(calls).toEqual([]);
});

test("a late reply for the tab left behind never shows on the new one", async () => {
  const { held } = core({ hold: true });
  const { feed, setTarget } = feedFor(A);
  await flush();
  setTarget(B);
  await flush();

  // B answers first, then A's older reply lands.
  held[1]?.();
  await flush();
  held[0]?.();
  await flush();

  const shown = feed.changes();
  expect(shown && "value" in shown ? shown.value : undefined).toMatchObject({
    name: String.raw`C:\b`,
  });
});
