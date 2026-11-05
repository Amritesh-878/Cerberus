import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";

import {
  activeTitle,
  cleanup,
  click,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { BridgeSnapshot } from "./messaging";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: String.raw`C:\code\p1` }],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: null,
      account: 0,
    },
    {
      id: 6,
      projectId: 1,
      parentId: null,
      title: "coder",
      program: "claude",
      cwd: null,
      account: 1,
    },
    {
      id: 7,
      projectId: 1,
      parentId: null,
      title: "shell",
      program: "powershell",
      cwd: null,
      account: 1,
    },
  ],
  nextId: 8,
};

function core(snapshot: BridgeSnapshot) {
  const pty = ptyCore();
  const decided: unknown[] = [];
  return {
    decided,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      bridge_snapshot: () => snapshot,
      bridge_decide: (args: unknown) => {
        decided.push([field(args, "id"), field(args, "decision")]);
        return { how: "delivered", from: 3, to: 6 };
      },
    },
  };
}

const views = (root: HTMLElement) =>
  [...root.querySelectorAll<HTMLElement>(".tab-view")].map((v) =>
    [...v.classList].filter((c) => c !== "tab-view").join(" "),
  );

test("an approved message shows both chats side by side on the target", async () => {
  const app = core({
    pending: [{ id: 1, from: 3, to: 6, text: "what is 2+2?" }],
    notes: [],
  });
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "coder");
  await open(root, "planner");

  expect(root.querySelector(".approval")?.textContent).toContain(
    "alpha / planner",
  );
  click(root, "Approve");
  await settle();

  expect(app.decided).toEqual([[1, { kind: "approve", text: "what is 2+2?" }]]);
  expect(activeTitle(root)).toBe("coder");
  expect(views(root)).toEqual(["shown split-left", "shown split-right"]);
  expect(root.querySelector(".split-bar")?.textContent).toContain(
    "alpha / planner and alpha / coder",
  );

  click(root, "Close split");
  await settle();
  expect(views(root)).toEqual(["", "shown"]);
});

test("showing another tab closes the split", async () => {
  const app = core({
    pending: [{ id: 1, from: 3, to: 6, text: "hi" }],
    notes: [],
  });
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "coder");
  await open(root, "planner");
  click(root, "Approve");
  await settle();

  await open(root, "shell");

  expect(root.querySelector(".split-bar")).toBeNull();
  expect(views(root).filter((v) => v.includes("split"))).toEqual([]);
});

test("closing the other half ends the split", async () => {
  const app = core({
    pending: [{ id: 1, from: 3, to: 6, text: "hi" }],
    notes: [],
  });
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "coder");
  await open(root, "planner");
  click(root, "Approve");
  await settle();

  click(root, "Close planner");
  await settle();

  expect(root.querySelector(".split-bar")).toBeNull();
  expect(views(root)).toEqual(["shown"]);
});

test("a tab's note says when messages are prefill only or off", async () => {
  const app = core({ pending: [], notes: [{ tab: 6, note: "prefillOnly" }] });
  const root = await renderApp(() => SAVED, app.core);

  await open(root, "coder");
  expect(root.querySelector(".messages-note")?.textContent).toBe(
    "messages: prefill only",
  );
  await open(root, "planner");
  expect(root.querySelector(".messages-note")).toBeNull();

  await emitEvent("bridge", { pending: [], notes: [{ tab: 3, note: "off" }] });
  await settle();
  expect(root.querySelector(".messages-note")?.textContent).toBe(
    "messages off",
  );
});

test("a new waiting message from the core shows in the popup", async () => {
  const app = core({ pending: [], notes: [] });
  const root = await renderApp(() => SAVED, app.core);
  expect(root.querySelector(".approval")).toBeNull();

  await emitEvent("bridge", {
    pending: [
      { id: 4, from: 6, to: 3, text: "4" },
      { id: 5, from: 6, to: 3, text: "and 5" },
    ],
    notes: [],
  });
  await settle();

  expect(root.querySelector(".approval-count")?.textContent).toBe("1 of 2");
  expect(root.querySelector<HTMLTextAreaElement>(".approval-text")?.value).toBe(
    "4",
  );
});
