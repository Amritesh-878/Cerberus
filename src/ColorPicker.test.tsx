import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { ColorPicker, swatchAfter } from "./ColorPicker";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

function picker(color: string | undefined, picks: string[]): HTMLElement {
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(
    () => (
      <ColorPicker
        id={0}
        name="work"
        color={color}
        onPick={(c) => picks.push(c)}
      />
    ),
    root,
  );
  return root;
}

function checked(root: HTMLElement): string[] {
  return [...root.querySelectorAll('[role=radio][aria-checked="true"]')].map(
    (b) => b.getAttribute("aria-label") ?? "",
  );
}

test("the account's colour is the one marked, by its name as well as its colour", () => {
  const root = picker("teal", []);

  expect(
    root.querySelector("[role=radiogroup]")?.getAttribute("aria-label"),
  ).toBe("Colour for work");
  expect(checked(root)).toEqual(["Teal"]);
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Teal");
});

test("an account with no saved colour shows the one its id gave it as chosen", () => {
  expect(checked(picker(undefined, []))).toEqual(["Terracotta"]);
});

test("clicking a swatch picks it", () => {
  const picks: string[] = [];
  const root = picker("teal", picks);

  root.querySelector<HTMLButtonElement>('[aria-label="Rose"]')?.click();

  expect(picks).toEqual(["rose"]);
});

test("arrow keys pick the next or previous swatch, wrapping at the ends", () => {
  const picks: string[] = [];
  const root = picker("silver", picks);
  const group = root.querySelector("[role=radiogroup]");

  group?.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
  );
  group?.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }),
  );

  expect(picks).toEqual(["terracotta", "teal"]);
});

test("Home and End jump to the ends, and other keys pick nothing", () => {
  expect(swatchAfter(3, "Home")).toBe(0);
  expect(swatchAfter(3, "End")).toBe(7);
  expect(swatchAfter(0, "ArrowUp")).toBe(7);
  expect(swatchAfter(3, "Enter")).toBeUndefined();
});
