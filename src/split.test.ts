import { createRoot, createSignal } from "solid-js";
import { expect, test } from "vitest";

import { createSplit, dropSide, nextPair } from "./split";

test("a reply between the same two tabs keeps their sides", () => {
  expect(nextPair(undefined, 3, 6)).toEqual([3, 6]);
  expect(nextPair([3, 6], 6, 3)).toEqual([3, 6]);
  expect(nextPair([3, 6], 3, 9)).toEqual([3, 9]);
});

test("the split shows while one of its tabs is shown, and comes back after another tab", () => {
  createRoot((dispose) => {
    const [shown, setShown] = createSignal<number | undefined>(3);
    const split = createSplit(shown);

    split.open(3, 6);
    expect(split.showing()).toEqual([3, 6]);
    expect(split.side(3)).toBe("left");
    expect(split.side(6)).toBe("right");
    expect(split.beside()).toBe(6);

    setShown(6);
    expect(split.has(3)).toBe(true);
    expect(split.beside()).toBe(3);

    setShown(9);
    expect(split.showing()).toBeUndefined();
    expect(split.beside()).toBeUndefined();
    setShown(undefined);
    expect(split.showing()).toBeUndefined();

    setShown(3);
    expect(split.showing()).toEqual([3, 6]);
    dispose();
  });
});

test("a tab never splits with itself", () => {
  createRoot((dispose) => {
    const split = createSplit(() => 3);

    split.open(3, 3);

    expect(split.showing()).toBeUndefined();
    dispose();
  });
});

test("a tab dropped on either half of the chat area lands on that half, and outside on none", () => {
  const area = { left: 200, top: 40, width: 800, height: 600 };

  expect(dropSide(600, 300, area)).toBe("right");
  expect(dropSide(1000, 640, area)).toBe("right");
  expect(dropSide(599, 300, area)).toBe("left");
  expect(dropSide(200, 40, area)).toBe("left");
  expect(dropSide(1001, 300, area)).toBeUndefined();
  expect(dropSide(700, 30, area)).toBeUndefined();
  expect(dropSide(150, 300, area)).toBeUndefined();
});

test("place puts a tab on the asked side of the shown one, and swap exchanges them", () => {
  createRoot((dispose) => {
    const split = createSplit(() => 3);

    split.place(5, "left");
    expect(split.showing()).toEqual([5, 3]);
    split.swap();
    expect(split.showing()).toEqual([3, 5]);
    split.place(3, "right");
    expect(split.showing()).toEqual([3, 5]);
    split.place(7, "right");
    expect(split.showing()).toEqual([3, 7]);
    dispose();
  });
});

test("closing either of its tabs ends it", () => {
  createRoot((dispose) => {
    const split = createSplit(() => 3);
    split.open(3, 6);

    split.forget(9);
    expect(split.has(6)).toBe(true);
    split.forget(6);

    expect(split.showing()).toBeUndefined();
    dispose();
  });
});

test("close ends it", () => {
  createRoot((dispose) => {
    const split = createSplit(() => 3);
    split.open(3, 6);

    split.close();

    expect(split.has(6)).toBe(false);
    dispose();
  });
});
