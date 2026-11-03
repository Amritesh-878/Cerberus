import { afterEach, beforeEach, expect, test } from "vitest";

import { blockReloads } from "./reloads";

let unblock: () => void;

beforeEach(() => {
  unblock = blockReloads(window, "other");
});

afterEach(() => {
  unblock();
  document.body.replaceChildren();
});

function press(target: EventTarget, key: string, ctrlKey = false): boolean {
  const event = new KeyboardEvent("keydown", {
    key,
    ctrlKey,
    bubbles: true,
    cancelable: true,
  });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

function rightClick(target: EventTarget): boolean {
  const event = new MouseEvent("contextmenu", {
    bubbles: true,
    cancelable: true,
  });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

function add<K extends keyof HTMLElementTagNameMap>(
  tag: K,
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  document.body.append(element);
  return element;
}

test("reload keys are cancelled wherever they are pressed", () => {
  const input = add("input");

  expect(press(document.body, "F5")).toBe(true);
  expect(press(input, "r", true)).toBe(true);
  expect(press(document.body, "t", true)).toBe(false);
});

test("the webview's menu is cancelled, but text boxes keep theirs", () => {
  const plain = add("div");
  const text = add("input");
  const area = add("textarea");
  const box = add("input");
  box.type = "checkbox";

  expect(rightClick(plain)).toBe(true);
  expect(rightClick(box)).toBe(true);
  expect(rightClick(text)).toBe(false);
  expect(rightClick(area)).toBe(false);
});

test("an app menu opened by a row still opens", () => {
  const row = add("div");
  let opened = false;
  row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    opened = true;
  });

  rightClick(row);

  expect(opened).toBe(true);
});

test("unblocking lets reloads through again", () => {
  unblock();

  expect(press(document.body, "F5")).toBe(false);
  expect(rightClick(add("div"))).toBe(false);
  unblock = blockReloads(window, "other");
});
