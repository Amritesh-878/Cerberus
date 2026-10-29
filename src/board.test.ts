import { expect, test } from "vitest";

import {
  type Board,
  exited,
  markOf,
  nextWaiting,
  opened,
  reported,
  resetText,
  startedTabs,
} from "./board";

const EMPTY: Board = new Map();
const WORKING = { state: "working" } as const;
const IDLE = { state: "idle" } as const;

function finishedOffScreen(board: Board, tab: number): Board {
  return reported(reported(board, tab, WORKING, undefined), tab, IDLE, 99);
}

test("a turn that ends while you are on another tab is waiting on you", () => {
  const board = reported(EMPTY, 1, WORKING, 2);
  expect(markOf(board, 1)).toEqual({ kind: "working" });

  expect(markOf(reported(board, 1, IDLE, 2), 1)).toEqual({ kind: "waiting" });
});

test("a turn that ends on the tab you are looking at waits for nobody", () => {
  const board = reported(reported(EMPTY, 1, WORKING, 1), 1, IDLE, 1);

  expect(markOf(board, 1)).toBeUndefined();
});

test("opening the tab clears waiting", () => {
  const board = opened(finishedOffScreen(EMPTY, 1), 1);

  expect(markOf(board, 1)).toBeUndefined();
});

test("a tab that was only ever idle is not waiting", () => {
  expect(markOf(reported(EMPTY, 1, IDLE, 2), 1)).toBeUndefined();
});

test("waiting holds through repeated idle reports and ends with a new turn", () => {
  const waiting = reported(finishedOffScreen(EMPTY, 1), 1, IDLE, 2);
  expect(markOf(waiting, 1)).toEqual({ kind: "waiting" });

  expect(markOf(reported(waiting, 1, WORKING, 2), 1)).toEqual({
    kind: "working",
  });
});

test("a limit shows with its reset time instead of waiting", () => {
  const board = reported(
    reported(EMPTY, 1, WORKING, 2),
    1,
    { state: "limited", resetsAt: 1_800_000_000 },
    2,
  );

  expect(markOf(board, 1)).toEqual({
    kind: "limited",
    resetsAt: 1_800_000_000,
  });
});

test("an exited program is done and no longer waiting", () => {
  const board = exited(finishedOffScreen(EMPTY, 1), 1);

  expect(markOf(board, 1)).toEqual({ kind: "done" });
});

test("the jump key goes to the next waiting tab in order and wraps", () => {
  const board = finishedOffScreen(finishedOffScreen(EMPTY, 2), 5);
  const order = [1, 2, 3, 5, 8];

  expect(nextWaiting(board, order, 1)).toBe(2);
  expect(nextWaiting(board, order, 2)).toBe(5);
  expect(nextWaiting(board, order, 5)).toBe(2);
  expect(nextWaiting(board, order, undefined)).toBe(2);
});

test("the jump key skips limited tabs and finds nothing when none wait", () => {
  const board = reported(
    reported(EMPTY, 3, WORKING, 1),
    3,
    { state: "limited", resetsAt: null },
    1,
  );

  expect(nextWaiting(board, [1, 3], 1)).toBeUndefined();
  expect(nextWaiting(finishedOffScreen(EMPTY, 1), [1], 1)).toBeUndefined();
});

test("restored tabs start nothing until they are shown", () => {
  const none = startedTabs(new Set(), undefined, [1, 2, 3]);
  expect([...none]).toEqual([]);

  const one = startedTabs(none, 2, [1, 2, 3]);
  expect([...one]).toEqual([2]);
  expect(startedTabs(one, 2, [1, 2, 3])).toBe(one);
});

test("a started tab keeps running when you move away and is dropped once closed", () => {
  const two = startedTabs(startedTabs(new Set(), 1, [1, 2]), 2, [1, 2]);
  expect([...two]).toEqual([1, 2]);

  expect([...startedTabs(two, undefined, [2])]).toEqual([2]);
});

test("a reset time reads like a clock", () => {
  const at = new Date(2026, 9, 29, 5, 50).getTime() / 1000;

  expect(resetText(at, "en-US")).toBe("5:50am");
});
