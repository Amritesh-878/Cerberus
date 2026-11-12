import { expect, test } from "vitest";

import { tabDrag, type TabDragEvents } from "./tabDrag";

function rowButton() {
  const button = document.createElement("button");
  const captured: number[] = [];
  button.setPointerCapture = (id) => captured.push(id);
  button.hasPointerCapture = () => captured.length > 0;
  button.releasePointerCapture = () => {
    captured.length = 0;
  };
  return { button, captured };
}

type Press = PointerEvent & { currentTarget: HTMLElement };

// Handlers read `currentTarget`, which only a dispatch sets, so it is pinned by hand.
function on(event: PointerEvent, button: HTMLElement): Press {
  Object.defineProperty(event, "currentTarget", { value: button });
  return event as Press;
}

function press(button: HTMLElement, x: number): Press {
  return on(
    new PointerEvent("pointerdown", { clientX: x, clientY: 10, button: 0 }),
    button,
  );
}

// A move while dragging holds the button down, as a real one does.
function at(type: string, button: HTMLElement, x: number): Press {
  const buttons = type === "pointermove" ? 1 : 0;
  return on(
    new PointerEvent(type, { clientX: x, clientY: 10, buttons }),
    button,
  );
}

function recorder() {
  const seen: string[] = [];
  const events: TabDragEvents = {
    move: (x) => seen.push(`move ${String(x)}`),
    drop: (x) => seen.push(`drop ${String(x)}`),
    cancel: () => seen.push("cancel"),
  };
  return { seen, events };
}

test("the press holds the pointer, so a quick flick off the row still drags", () => {
  const { button, captured } = rowButton();
  const drag = tabDrag();

  drag.onPointerDown(press(button, 10));

  expect(captured.length).toBe(1);
});

test("a finger leaves the list to scroll and never drags a tab", () => {
  const { button, captured } = rowButton();
  const { seen, events } = recorder();
  const drag = tabDrag();

  drag.onPointerDown(
    on(
      new PointerEvent("pointerdown", {
        clientX: 10,
        clientY: 10,
        button: 0,
        pointerType: "touch",
      }),
      button,
    ),
  );
  drag.onPointerMove(at("pointermove", button, 400), events);

  expect(captured).toEqual([]);
  expect(seen).toEqual([]);
});

test("a wobble under a few pixels is a click, not a drag", () => {
  const { button } = rowButton();
  const { seen, events } = recorder();
  const drag = tabDrag();

  drag.onPointerDown(press(button, 10));
  drag.onPointerMove(at("pointermove", button, 13), events);
  drag.onPointerUp(at("pointerup", button, 13), events);

  expect(seen).toEqual([]);
  expect(drag.swallowClick()).toBe(false);
});

test("a drop reports where it ended and swallows only the click after it", () => {
  const { button, captured } = rowButton();
  const { seen, events } = recorder();
  const drag = tabDrag();

  drag.onPointerDown(press(button, 10));
  drag.onPointerMove(at("pointermove", button, 400), events);
  drag.onPointerUp(at("pointerup", button, 600), events);

  expect(seen).toEqual(["move 400", "drop 600"]);
  expect(captured).toEqual([]);
  expect(drag.swallowClick()).toBe(true);
  expect(drag.swallowClick()).toBe(false);
});

test("a cancelled drag says so and drops nothing", () => {
  const { button } = rowButton();
  const { seen, events } = recorder();
  const drag = tabDrag();

  drag.onPointerDown(press(button, 10));
  drag.onPointerMove(at("pointermove", button, 400), events);
  drag.onPointerCancel(at("pointercancel", button, 400), events);

  expect(seen).toEqual(["move 400", "cancel"]);
  expect(drag.swallowClick()).toBe(false);
});

test.each([
  [
    "the window loses focus",
    () => window.dispatchEvent(new FocusEvent("blur")),
  ],
  [
    "a key is pressed",
    () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" })),
  ],
  [
    "the mouse moves with no button held",
    () => window.dispatchEvent(new MouseEvent("mousemove", { buttons: 0 })),
  ],
  [
    "a pointer moves with no button held",
    () => window.dispatchEvent(new PointerEvent("pointermove", { buttons: 0 })),
  ],
  ["a native drag ends", () => window.dispatchEvent(new Event("dragend"))],
  ["something is dropped", () => window.dispatchEvent(new Event("drop"))],
  [
    "the page is hidden",
    () => {
      Object.defineProperty(document, "hidden", {
        value: true,
        configurable: true,
      });
      document.dispatchEvent(new Event("visibilitychange"));
      Object.defineProperty(document, "hidden", {
        value: false,
        configurable: true,
      });
    },
  ],
])("a drag whose release never comes ends when %s", (_name, happen) => {
  const { button, captured } = rowButton();
  const { seen, events } = recorder();
  const drag = tabDrag();
  drag.onPointerDown(press(button, 10));
  drag.onPointerMove(at("pointermove", button, 400), events);

  happen();
  drag.onPointerMove(at("pointermove", button, 500), events);
  window.dispatchEvent(new FocusEvent("blur"));

  expect(seen).toEqual(["move 400", "cancel"]);
  expect(captured).toEqual([]);
  expect(drag.swallowClick()).toBe(false);
});

test("a move with the button held keeps the drag, and the row's own move with none ends it", () => {
  const { button } = rowButton();
  const { seen, events } = recorder();
  const drag = tabDrag();
  drag.onPointerDown(press(button, 10));
  drag.onPointerMove(at("pointermove", button, 400), events);

  window.dispatchEvent(new MouseEvent("mousemove", { buttons: 1 }));
  drag.onPointerMove(
    on(new PointerEvent("pointermove", { clientX: 450, buttons: 0 }), button),
    events,
  );

  expect(seen).toEqual(["move 400", "cancel"]);
});
