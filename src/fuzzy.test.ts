import { expect, test } from "vitest";

import { fuzzyWords, letterRanges, wordsOf } from "./fuzzy";

test("letters in order match and mark where they landed, as the core marks chat names", () => {
  expect(fuzzyWords(["cerpl"], "Cerberus Planner")?.marks).toEqual([
    0, 1, 2, 9, 10,
  ]);
  expect(fuzzyWords(["lpc"], "Cerberus Planner")).toBeNull();
  expect(fuzzyWords([], "Cerberus Planner")).toBeNull();
});

test("a run at the start beats a word start, which beats letters scattered through the middle", () => {
  const score = (name: string) => fuzzyWords(["plan"], name)?.score ?? 0;

  expect(score("Planner notes")).toBeGreaterThan(score("the planner"));
  expect(score("the planner")).toBeGreaterThan(score("spelling a nap"));
});

test("every word must match, a camel-case hump is a word start, and ranges join neighbours", () => {
  expect(fuzzyWords(wordsOf("cer ZEBRA"), "Cerberus Planner")).toBeNull();
  expect(fuzzyWords(["fc"], "FindChat")?.score ?? 0).toBeGreaterThan(
    fuzzyWords(["fc"], "Fiche")?.score ?? 0,
  );
  expect(letterRanges([0, 1, 2, 9, 10])).toEqual([
    [0, 3],
    [9, 2],
  ]);
});
