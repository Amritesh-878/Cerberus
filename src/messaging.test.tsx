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
    senderGone: [],
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

const TWO_PROJECTS: SavedTree = {
  ...TREE,
  projects: [
    ...TREE.projects,
    { id: 2, name: "beta", folder: String.raw`C:\code\p2` },
  ],
  tabs: [
    ...TREE.tabs,
    {
      id: 8,
      projectId: 2,
      parentId: null,
      title: "other",
      program: "claude",
      cwd: null,
      account: 0,
    },
  ],
  nextId: 9,
};

const splitOf = (root: HTMLElement) =>
  root.querySelector(".split-bar") === null
    ? "none"
    : views(root)
        .map((v) =>
          v.includes("split-left")
            ? "L"
            : v.includes("split-right")
              ? "R"
              : "-",
        )
        .join("");

async function arrive(id: number, from: number, to: number): Promise<void> {
  await emitEvent("bridge", {
    pending: [{ id, from, to, text: "hi" }],
    notes: [],
    senderGone: [],
  });
  await settle();
}

async function gone(): Promise<void> {
  await emitEvent("bridge", { pending: [], notes: [], senderGone: [] });
  await settle();
}

test("every delivered message opens or keeps its split, nothing else changes it, and it outlives a project switch", async () => {
  const replies: unknown[] = [];
  const pty = ptyCore();
  const root = await renderApp(() => SAVED, {
    ...pty.core,
    tree_load: () => ({ tree: TWO_PROJECTS, notice: null }),
    bridge_snapshot: () => ({ pending: [], notes: [], senderGone: [] }),
    bridge_decide: () => replies.shift(),
    bridge_type_in: () => null,
    bridge_discard: () => null,
  });
  await open(root, "planner");
  // Views in tree order: planner, coder, shell, other; only started tabs have one.

  replies.push({ how: "delivered", from: 3, to: 6 });
  await arrive(1, 3, 6);
  click(root, "Approve");
  await settle();
  await gone();
  expect(activeTitle(root)).toBe("coder");
  expect(splitOf(root)).toBe("LR");

  await open(root, "other");
  expect(splitOf(root)).toBe("none");
  await open(root, "shell");
  expect(splitOf(root)).toBe("none");
  await open(root, "planner");
  expect(splitOf(root)).toBe("LR--");

  click(root, "Close split");
  await settle();
  replies.push({ how: "rejected" });
  await arrive(2, 6, 3);
  click(root, "Reject");
  await settle();
  await gone();
  expect(splitOf(root)).toBe("none");

  replies.push({ how: "failed", reason: "the tab closed (fake)" });
  await arrive(3, 3, 6);
  click(root, "Approve");
  await settle();
  click(root, "OK");
  await gone();
  expect(splitOf(root)).toBe("none");

  // Cancelled by the sender: the message just leaves the queue.
  await arrive(4, 3, 6);
  await gone();
  expect(splitOf(root)).toBe("none");

  // Edited, then typed into the target's input box for Enter.
  replies.push({ how: "prefilled", from: 3, to: 6 });
  await arrive(5, 3, 6);
  const box = root.querySelector<HTMLTextAreaElement>(".approval-text");
  if (box) box.value = "hello";
  box?.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  click(root, "Approve edited");
  await settle();
  click(root, "OK");
  await gone();
  expect(splitOf(root)).toBe("LR--");

  // A reply keeps the sides; typed in after the split was closed, it opens again with the sender on the left.
  replies.push({ how: "unconfirmed", id: 6, from: 6, to: 3 });
  await arrive(6, 6, 3);
  click(root, "Approve");
  await settle();
  await gone();
  expect(activeTitle(root)).toBe("planner");
  expect(splitOf(root)).toBe("LR--");
  click(root, "Close split");
  await settle();
  click(root, "Type it in instead");
  await settle();
  expect(splitOf(root)).toBe("RL--");
});

test("closing the other half ends the split", async () => {
  const app = core({
    pending: [{ id: 1, from: 3, to: 6, text: "hi" }],
    notes: [],
    senderGone: [],
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
  const app = core({
    pending: [],
    notes: [{ tab: 6, note: "prefillOnly" }],
    senderGone: [],
  });
  const root = await renderApp(() => SAVED, app.core);

  await open(root, "coder");
  expect(root.querySelector(".messages-note")?.textContent).toBe(
    "messages: prefill only",
  );
  await open(root, "planner");
  expect(root.querySelector(".messages-note")).toBeNull();

  await emitEvent("bridge", {
    pending: [],
    notes: [{ tab: 3, note: "off" }],
    senderGone: [],
  });
  await settle();
  expect(root.querySelector(".messages-note")?.textContent).toBe(
    "messages off",
  );
});

test("a new waiting message from the core shows in the popup", async () => {
  const app = core({ pending: [], notes: [], senderGone: [] });
  const root = await renderApp(() => SAVED, app.core);
  expect(root.querySelector(".approval")).toBeNull();

  await emitEvent("bridge", {
    pending: [
      { id: 4, from: 6, to: 3, text: "4" },
      { id: 5, from: 6, to: 3, text: "and 5" },
    ],
    notes: [],
    senderGone: [],
  });
  await settle();

  expect(root.querySelector(".approval-count")?.textContent).toBe("1 of 2");
  expect(root.querySelector<HTMLTextAreaElement>(".approval-text")?.value).toBe(
    "4",
  );
});

test("the popup says so when the chat that sent its message is gone", async () => {
  const app = core({
    pending: [{ id: 1, from: 3, to: 6, text: "hi" }],
    notes: [],
    senderGone: [],
  });
  const root = await renderApp(() => SAVED, app.core);
  expect(root.querySelector(".approval-text")).not.toBeNull();

  await emitEvent("bridge", { pending: [], notes: [], senderGone: [1] });
  await settle();

  expect(root.querySelector(".approval")?.textContent).toContain(
    "The sending chat is gone",
  );
  expect(app.decided).toEqual([]);
});
