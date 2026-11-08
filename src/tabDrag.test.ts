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

function at(type: string, button: HTMLElement, x: number): Press {
  return on(new PointerEvent(type, { clientX: x, clientY: 10 }), button);
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
