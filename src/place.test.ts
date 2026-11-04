import { expect, test } from "vitest";

import { onScreen, placeBox } from "./place";

const BOX = { width: 280, height: 300 };
const SCREEN = { width: 1280, height: 800 };

// A 26 px button, like the + beside a project.
function button(left: number, top: number) {
  return { left, right: left + 26, top, bottom: top + 26 };
}

test("the box opens below its anchor when it fits", () => {
  expect(placeBox(button(200, 60), BOX, SCREEN)).toEqual({
    left: 200,
    top: 90,
  });
});

test("near the bottom the box opens above its anchor", () => {
  expect(placeBox(button(200, 700), BOX, SCREEN)).toEqual({
    left: 200,
    top: 396,
  });
});

test("when neither side fits the box sits as low as it can without leaving the window", () => {
  const short = { width: 1280, height: 400 };

  expect(placeBox(button(200, 150), BOX, short)).toEqual({
    left: 200,
    top: 92,
  });
});

test("the box stays inside the window's right edge", () => {
  expect(placeBox(button(1200, 60), BOX, SCREEN).left).toBe(992);
});

test("with no anchor the box opens centred near the top", () => {
  expect(placeBox(undefined, BOX, SCREEN)).toEqual({ left: 500, top: 64 });
});

test("with no anchor in a short window the box moves up to fit", () => {
  const short = { width: 1280, height: 340 };

  expect(placeBox(undefined, BOX, short).top).toBe(32);
  expect(placeBox(undefined, BOX, { width: 1280, height: 200 }).top).toBe(8);
});

test("beside a button the box opens to its right, level with it", () => {
  expect(placeBox(button(214, 58), BOX, SCREEN, "beside")).toEqual({
    left: 244,
    top: 58,
  });
});

test("beside a button near the right edge the box opens to its left", () => {
  expect(placeBox(button(1100, 58), BOX, SCREEN, "beside")).toEqual({
    left: 816,
    top: 58,
  });
});

test("beside a button near the bottom the box moves up to stay in the window", () => {
  expect(placeBox(button(214, 700), BOX, SCREEN, "beside").top).toBe(492);
});

test("in a window too small for either side the box stays inside it", () => {
  const tiny = { width: 400, height: 240 };

  expect(placeBox(button(150, 100), BOX, tiny, "beside")).toEqual({
    left: 112,
    top: 8,
  });
  expect(placeBox(button(150, 100), BOX, tiny)).toEqual({ left: 112, top: 8 });
});

test("a trigger in a closed drawer or outside the window is not on screen", () => {
  const rect = (left: number, width: number) => ({
    ...button(left, 58),
    right: left + width,
    width,
    height: 26,
  });

  expect(onScreen(rect(214, 26), SCREEN)).toBe(true);
  expect(onScreen(rect(-60, 26), SCREEN)).toBe(false);
  expect(onScreen(rect(214, 0), SCREEN)).toBe(false);
});

test("beside an anchor left outside a narrowed window the box still stays inside it", () => {
  const narrow = { width: 700, height: 800 };

  expect(placeBox(button(1200, 58), BOX, narrow, "beside").left).toBe(412);
});
