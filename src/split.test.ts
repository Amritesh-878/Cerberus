import { createRoot, createSignal } from "solid-js";
import { expect, test } from "vitest";

import { createSplit, dropsToSplit, nextPair } from "./split";

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

test("a tab dropped on the right half of the chat area splits", () => {
  const area = { left: 200, top: 40, width: 800, height: 600 };

  expect(dropsToSplit(600, 300, area)).toBe(true);
  expect(dropsToSplit(1000, 640, area)).toBe(true);
  expect(dropsToSplit(599, 300, area)).toBe(false);
  expect(dropsToSplit(1001, 300, area)).toBe(false);
  expect(dropsToSplit(700, 30, area)).toBe(false);
  expect(dropsToSplit(150, 300, area)).toBe(false);
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
