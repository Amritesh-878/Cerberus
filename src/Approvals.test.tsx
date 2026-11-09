import { batch, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { Approvals, type TabName } from "./Approvals";
import { button, click, settle } from "./harness";
import type { Decided, Decision, PendingMessage } from "./messaging";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

const NAMES: Partial<Record<number, TabName>> = {
  3: {
    accountId: 0,
    account: "personal",
    color: undefined,
    place: "alpha / planner",
  },
  6: { accountId: 1, account: "work", color: undefined, place: "beta / coder" },
};

const UNKNOWN: TabName = {
  accountId: undefined,
  account: "Default account",
  color: undefined,
  place: "tab ?",
};

const message = (id: number, text: string): PendingMessage => ({
  id,
  from: 3,
  to: 6,
  text,
});

function show(
  first: PendingMessage[],
  answer: Decided = { how: "delivered", from: 3, to: 6 },
) {
  const decisions: [number, Decision][] = [];
  const approved: [number, number][] = [];
  const typed: number[] = [];
  const dropped: number[] = [];
  const [pending, setPending] = createSignal(first);
  const [senderGone, setSenderGone] = createSignal<number[]>([]);
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(
    () => (
      <Approvals
        pending={pending()}
        senderGone={senderGone()}
        name={(tab) => NAMES[tab] ?? UNKNOWN}
        decide={(id, decision) => {
          decisions.push([id, decision]);
          // The core takes the message out of the queue once it is decided.
          setPending((all) => all.filter((m) => m.id !== id));
          return Promise.resolve(answer);
        }}
        typeIn={(id) => {
          typed.push(id);
          return Promise.resolve();
        }}
        discard={(id) => {
          dropped.push(id);
          return Promise.resolve();
        }}
        onApproved={(from, to) => approved.push([from, to])}
      />
    ),
    root,
  );
  // One core snapshot carries both, so they change together.
  const senderLeft = (id: number) => {
    batch(() => {
      setSenderGone((all) => [...all, id]);
      setPending((all) => all.filter((m) => m.id !== id));
    });
  };
  return { root, decisions, approved, typed, dropped, setPending, senderLeft };
}

function textBox(root: HTMLElement): HTMLTextAreaElement {
  const box = root.querySelector<HTMLTextAreaElement>(".approval-text");
  if (!box) throw new Error("no message box");
  return box;
}

test("the popup shows both tabs and the whole message", () => {
  const { root } = show([message(1, "what is 2+2?\nshow your work")]);

  const lines = [...root.querySelectorAll(".approval-tab")].map(
    (l) => l.textContent,
  );
  expect(lines).toEqual([
    "Frompersonal alpha / planner",
    "Towork beta / coder",
  ]);
  expect(textBox(root).value).toBe("what is 2+2?\nshow your work");
  expect(root.querySelector(".approval-count")).toBeNull();
  // Account 0 keeps its colour dot too.
  expect(root.querySelectorAll(".approval-tab .dot[style]")).toHaveLength(2);
});

test("approve sends the text as it was and shows both chats", async () => {
  const { root, decisions, approved } = show([message(1, "hi")]);

  click(root, "Approve");
  await settle();

  expect(decisions).toEqual([[1, { kind: "approve", text: "hi" }]]);
  expect(approved).toEqual([[3, 6]]);
  expect(root.querySelector(".approval")).toBeNull();
});

test("an edit is sent as edited and the button says so", async () => {
  const { root, decisions } = show([message(1, "a long question")]);
  const box = textBox(root);

  box.value = "shorter";
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  await settle();

  expect(root.querySelector(".approval-edited")?.textContent).toContain(
    "Edited",
  );
  click(root, "Approve edited");
  await settle();
  expect(decisions).toEqual([[1, { kind: "approve", text: "shorter" }]]);
});

test("reject sends nothing on and opens no split", async () => {
  const { root, decisions, approved } = show([message(1, "hi")], {
    how: "rejected",
  });

  click(root, "Reject");
  await settle();

  expect(decisions).toEqual([[1, { kind: "reject" }]]);
  expect(approved).toEqual([]);
});

test("a message whose sending chat is gone says so, then the next shows", async () => {
  const { root, decisions, senderLeft } = show([
    message(1, "hi"),
    message(2, "next"),
  ]);

  senderLeft(1);
  await settle();

  expect(root.querySelector(".approval")?.textContent).toContain(
    "The sending chat is gone, so its message was dropped.",
  );
  click(root, "OK");
  await settle();
  expect(textBox(root).value).toBe("next");
  expect(decisions).toEqual([]);
});

test("a gone sender's message behind the shown one leaves quietly", async () => {
  const { root, senderLeft } = show([message(1, "hi"), message(2, "next")]);

  senderLeft(2);
  await settle();

  expect(textBox(root).value).toBe("hi");
  expect(root.querySelector(".approval-count")).toBeNull();
});

test("waiting messages queue in order with a count", async () => {
  const { root } = show([message(1, "first"), message(2, "second")]);

  expect(root.querySelector(".approval-count")?.textContent).toBe("1 of 2");
  click(root, "Reject");
  await settle();

  expect(textBox(root).value).toBe("second");
  expect(root.querySelector(".approval-count")).toBeNull();
});

test("escape and enter in the box never approve", async () => {
  const { root, decisions } = show([message(1, "hi")]);
  const box = textBox(root);

  for (const key of ["Escape", "Enter"]) {
    box.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  }
  await settle();

  expect(decisions).toEqual([]);
  expect(root.querySelector(".approval")).not.toBeNull();
});

test("enter on a focused approve button does not approve", async () => {
  const { root, decisions } = show([message(1, "hi")]);
  const approve = button(root, "Approve");
  approve.focus();

  const enter = new KeyboardEvent("keydown", {
    key: "Enter",
    bubbles: true,
    cancelable: true,
  });
  approve.dispatchEvent(enter);
  await settle();

  expect(enter.defaultPrevented).toBe(true);
  expect(decisions).toEqual([]);
});

test("an empty message cannot be approved", async () => {
  const { root } = show([message(1, "hi")]);
  const box = textBox(root);

  box.value = "  ";
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  await settle();

  expect(button(root, "Approve edited").disabled).toBe(true);
});

test("a typed-in message says it waits for enter until dismissed, then the next shows", async () => {
  const { root } = show([message(1, "hi"), message(2, "next")], {
    how: "prefilled",
    from: 3,
    to: 6,
  });

  click(root, "Approve");
  await settle();

  expect(root.querySelector(".approval")?.textContent).toContain(
    "It's in the input box of work · beta / coder. Press Enter there to send it.",
  );
  click(root, "OK");
  await settle();
  expect(textBox(root).value).toBe("next");
});

test("a failed delivery says why", async () => {
  const { root } = show([message(1, "hi")], {
    how: "failed",
    reason: "the target tab is closed",
  });

  click(root, "Approve");
  await settle();

  expect(root.querySelector(".approval")?.textContent).toContain(
    "Not delivered: the target tab is closed",
  );
});

test("an inbox message that hasn't shown is typed in only when asked", async () => {
  const { root, approved, typed } = show([message(1, "hi")], {
    how: "unconfirmed",
    id: 1,
    from: 3,
    to: 6,
  });

  click(root, "Approve");
  await settle();

  expect(approved).toEqual([[3, 6]]);
  expect(typed).toEqual([]);
  expect(root.querySelector(".approval")?.textContent).toContain(
    "It went to the inbox of work · beta / coder, but that tab hasn't shown it yet.",
  );
  click(root, "Type it in instead");
  await settle();

  expect(typed).toEqual([1]);
  const said = root.querySelector(".approval")?.textContent;
  expect(said).toContain(
    "It's in the input box of work · beta / coder. Press Enter there to send it.",
  );
  expect(said).toContain("If the first copy shows up there too");
});

test("closing an unshown message's notice drops its kept text", async () => {
  const { root, typed, dropped } = show([message(1, "hi")], {
    how: "unconfirmed",
    id: 1,
    from: 3,
    to: 6,
  });

  click(root, "Approve");
  await settle();
  click(root, "OK");
  await settle();

  expect(dropped).toEqual([1]);
  expect(typed).toEqual([]);
  expect(root.querySelector(".approval")).toBeNull();
});
