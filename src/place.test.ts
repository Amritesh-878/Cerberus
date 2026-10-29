import { expect, test } from "vitest";

import { placeBox } from "./place";

const BOX = { width: 280, height: 300 };
const SCREEN = { width: 1280, height: 800 };

test("the box opens below its anchor when it fits", () => {
  expect(placeBox({ left: 200, top: 60, bottom: 86 }, BOX, SCREEN)).toEqual({
    left: 200,
    top: 90,
  });
});

test("near the bottom the box opens above its anchor", () => {
  expect(placeBox({ left: 200, top: 700, bottom: 726 }, BOX, SCREEN)).toEqual({
    left: 200,
    top: 396,
  });
});

test("when neither side fits the box sits as low as it can without leaving the window", () => {
  const short = { width: 1280, height: 400 };

  expect(placeBox({ left: 200, top: 150, bottom: 176 }, BOX, short)).toEqual({
    left: 200,
    top: 92,
  });
});

test("the box stays inside the window's right edge", () => {
  expect(placeBox({ left: 1200, top: 60, bottom: 86 }, BOX, SCREEN).left).toBe(
    992,
  );
});

test("with no anchor the box opens centred near the top", () => {
  expect(placeBox(undefined, BOX, SCREEN)).toEqual({ left: 500, top: 64 });
});
