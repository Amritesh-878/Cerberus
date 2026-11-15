import { afterEach, expect, test } from "vitest";

import { keepInside } from "./ShortcutsSheet";

afterEach(() => {
  document.body.innerHTML = "";
});

function sheet(): HTMLElement {
  const box = document.createElement("div");
  box.innerHTML = [
    '<button id="cancel">Cancel</button>',
    '<details><summary id="summary">Task prompt</summary><textarea id="hidden"></textarea></details>',
    '<select id="base"><option>main</option></select>',
    '<button id="open" disabled>Open</button>',
  ].join("");
  document.body.append(box);
  return box;
}

function tab(shift = false): KeyboardEvent {
  return new KeyboardEvent("keydown", { key: "Tab", shiftKey: shift });
}

const focused = () => document.activeElement?.id;

test("Tab wraps from the last enabled control, skipping a disabled Open", () => {
  const box = sheet();
  document.getElementById("base")?.focus();

  expect(keepInside(box, tab())).toBe(true);
  expect(focused()).toBe("cancel");
});

test("Shift+Tab from the first control wraps to the last enabled one, a select included", () => {
  const box = sheet();
  document.getElementById("cancel")?.focus();

  expect(keepInside(box, tab(true))).toBe(true);
  expect(focused()).toBe("base");
});

test("a closed section's fields are not stops, but its summary is", () => {
  const box = sheet();
  document.getElementById("summary")?.focus();

  expect(keepInside(box, tab())).toBe(false);
  box.querySelector("details")?.setAttribute("open", "");
  document.getElementById("hidden")?.focus();
  expect(keepInside(box, tab())).toBe(false);
});
