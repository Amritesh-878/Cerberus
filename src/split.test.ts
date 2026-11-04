import { createRoot, createSignal } from "solid-js";
import { expect, test } from "vitest";

import { createSplit, nextPair } from "./split";

test("a reply between the same two tabs keeps their sides", () => {
  expect(nextPair(undefined, 3, 6)).toEqual([3, 6]);
  expect(nextPair([3, 6], 6, 3)).toEqual([3, 6]);
  expect(nextPair([3, 6], 3, 9)).toEqual([3, 9]);
});

test("the split shows while one of its tabs is shown and closes for any other", async () => {
  await createRoot(async (dispose) => {
    const [shown, setShown] = createSignal<number | undefined>(3);
    const split = createSplit(shown);

    split.open(3, 6);
    expect(split.showing()).toEqual([3, 6]);
    expect(split.side(3)).toBe("left");
    expect(split.side(6)).toBe("right");

    setShown(6);
    await Promise.resolve();
    expect(split.has(3)).toBe(true);

    setShown(9);
    await Promise.resolve();
    setShown(3);
    await Promise.resolve();
    expect(split.showing()).toBeUndefined();
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
