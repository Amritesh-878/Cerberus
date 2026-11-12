import { expect, test } from "vitest";

import { backdropDismiss } from "./backdrop";

function setup() {
  const backdrop = document.createElement("div");
  const dialog = document.createElement("div");
  backdrop.append(dialog);
  document.body.append(backdrop);
  let closed = 0;
  const handlers = backdropDismiss(() => {
    closed++;
  });
  backdrop.addEventListener("pointerdown", handlers.onPointerDown);
  backdrop.addEventListener("pointerup", handlers.onPointerUp);
  backdrop.addEventListener("click", handlers.onClick);
  const press = (el: Element) =>
    el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  // The click goes where a browser sends it: the common ancestor of the press and the release.
  const release = (el: Element, clickOn: Element = el) => {
    el.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
    clickOn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  };
  return { backdrop, dialog, press, release, closed: () => closed };
}

test("only a press and a release both on the backdrop close it", () => {
  const s = setup();

  s.press(s.dialog);
  s.release(s.backdrop);
  expect(s.closed()).toBe(0);

  s.press(s.backdrop);
  s.release(s.dialog, s.backdrop);
  expect(s.closed()).toBe(0);

  s.press(s.backdrop);
  s.release(s.dialog);
  expect(s.closed()).toBe(0);

  s.release(s.backdrop);
  expect(s.closed()).toBe(0);

  s.press(s.backdrop);
  s.release(s.backdrop);
  expect(s.closed()).toBe(1);
  s.backdrop.remove();
});
