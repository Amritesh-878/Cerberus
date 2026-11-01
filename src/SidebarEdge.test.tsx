import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { SidebarEdge } from "./SidebarEdge";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

function edgeAt(windowWidth: number, width: number) {
  Object.defineProperty(window, "innerWidth", {
    value: windowWidth,
    configurable: true,
  });
  const widths: number[] = [];
  let done = 0;
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(
    () => (
      <SidebarEdge
        width={width}
        onWidth={(w) => widths.push(w)}
        onDone={() => (done += 1)}
      />
    ),
    root,
  );
  const edge = root.querySelector<HTMLElement>(".sidebar-edge");
  if (!edge) throw new Error("no edge");
  return { edge, widths, done: () => done };
}

test("the edge reports the width the window lets it show, and its limits", () => {
  const { edge } = edgeAt(900, 560);

  expect(edge.getAttribute("aria-valuenow")).toBe("450");
  expect(edge.getAttribute("aria-valuemin")).toBe("200");
  expect(edge.getAttribute("aria-valuemax")).toBe("450");
});

test("an arrow key steps from the width shown, not a wider saved one", () => {
  const { edge, widths, done } = edgeAt(900, 560);

  edge.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }),
  );

  expect(widths).toEqual([434]);
  expect(done()).toBe(1);
});

test("a cancelled drag is saved like a finished one", () => {
  const { edge, widths, done } = edgeAt(1280, 248);
  edge.setPointerCapture = () => undefined;
  edge.hasPointerCapture = () => false;

  edge.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  edge.dispatchEvent(
    new PointerEvent("pointermove", { bubbles: true, clientX: 300 }),
  );
  edge.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true }));
  edge.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));

  expect(widths).toEqual([300]);
  expect(done()).toBe(1);
});
