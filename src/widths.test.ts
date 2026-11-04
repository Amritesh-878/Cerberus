import { expect, test } from "vitest";

import { clampPanel, clampSidebar } from "./widths";

test("the sidebar keeps a width inside its limits", () => {
  expect(clampSidebar(320, 1280)).toBe(320);
  expect(clampSidebar(120, 1280)).toBe(200);
  expect(clampSidebar(900, 1600)).toBe(560);
});

test("the sidebar never takes more than half the window", () => {
  expect(clampSidebar(500, 900)).toBe(450);
  expect(clampSidebar(300, 300)).toBe(200);
});

test("the panel keeps a width inside its limits and two fifths of the window", () => {
  expect(clampPanel(300, 1280)).toBe(300);
  expect(clampPanel(100, 1280)).toBe(240);
  expect(clampPanel(900, 1280)).toBe(512);
  expect(clampPanel(900, 2000)).toBe(640);
  expect(clampPanel(300, 400)).toBe(240);
});
