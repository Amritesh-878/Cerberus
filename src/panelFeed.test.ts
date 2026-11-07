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

const A: Target = { tab: 1, folder: String.raw`C:\a`, account: 1, dirs: [] };
const B: Target = { tab: 2, folder: String.raw`C:\b`, account: 2, dirs: [] };

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
          // Folders under C:\a share its repo, as a subfolder would.
          root: folder.startsWith(String.raw`C:\a`) ? String.raw`C:\a` : folder,
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
      case "panel_prs":
        return [];
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
  const [links, setLinks] = createSignal(0);
  const feed = createRoot((stop) => {
    dispose = stop;
    return createPanelFeed(target, POLL_MS, links);
  });
  return {
    feed,
    setTarget,
    linkChanged: () => setLinks((n) => n + 1),
  };
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
    String.raw`panel_prs C:\a`,
    String.raw`panel_skills C:\a`,
  ]);

  calls.length = 0;
  await vi.advanceTimersByTimeAsync(POLL_MS);

  expect(calls.sort()).toEqual([
    String.raw`panel_changes C:\a`,
    String.raw`panel_prs C:\a`,
  ]);
});

test("each extra folder is read, and a repo's PRs once however many of its folders", async () => {
  const { calls } = core();
  const { feed } = feedFor({
    ...A,
    dirs: [String.raw`C:\a\docs`, String.raw`C:\plans`],
  });
  await flush();

  expect(calls.filter((c) => c.startsWith("panel_changes "))).toEqual([
    String.raw`panel_changes C:\a`,
    String.raw`panel_changes C:\a\docs`,
    String.raw`panel_changes C:\plans`,
  ]);
  expect(calls.filter((c) => c.startsWith("panel_prs ")).sort()).toEqual([
    String.raw`panel_prs C:\a`,
    String.raw`panel_prs C:\plans`,
  ]);
  const prs = feed.prs();
  expect(prs && "value" in prs ? prs.value.map((r) => r.root) : []).toEqual([
    String.raw`C:\a`,
    String.raw`C:\plans`,
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
  expect(feed.folders()).toBeUndefined();
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

  const shown = feed.folders();
  expect(
    shown && "value" in shown ? shown.value[0]?.changes : undefined,
  ).toMatchObject({
    name: String.raw`C:\b`,
  });
});

test("a tab on the same folder and account still reloads the folder's files", async () => {
  const { calls } = core();
  const { setTarget } = feedFor(A);
  await flush();

  calls.length = 0;
  setTarget({ ...A, tab: 9 });
  await flush();

  expect(calls.filter((c) => c.startsWith("panel_connectors"))).toHaveLength(1);
  expect(calls.filter((c) => c.startsWith("panel_skills"))).toHaveLength(1);
});

test("a chat linking or leaving the bridge reloads only the connectors", async () => {
  const { calls } = core();
  const { linkChanged } = feedFor(A);
  await flush();

  calls.length = 0;
  linkChanged();
  await flush();

  expect(calls).toEqual([String.raw`panel_connectors C:\a`]);
});

test("a git reply that lands after the panel closes starts no PR read", async () => {
  const { calls, held } = core({ hold: true });
  feedFor(A);
  await flush();
  dispose?.();
  dispose = undefined;

  held[0]?.();
  await flush();

  expect(calls.filter((c) => c.startsWith("panel_pr"))).toEqual([]);
});

test("the left tab's late git reply starts no PR read for it", async () => {
  const { calls, held } = core({ hold: true });
  const { setTarget } = feedFor(A);
  await flush();
  setTarget(B);
  await flush();

  held[1]?.();
  await flush();
  held[0]?.();
  await flush();

  const prs = calls.filter((c) => c.startsWith("panel_pr"));
  expect(prs).toHaveLength(1);
  expect(prs[0]?.endsWith("b")).toBe(true);
});
