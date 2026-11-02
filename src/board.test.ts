import { expect, test } from "vitest";

import {
  alertFor,
  type Board,
  type Change,
  cleared,
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

test("a stopped tab's state is forgotten", () => {
  const board = cleared(finishedOffScreen(EMPTY, 1), 1);

  expect(markOf(board, 1)).toBeUndefined();
  expect(cleared(board, 1)).toBe(board);
});

const AWAY = { shown: 2, focused: true, alertsOn: true };
const LIMIT = { state: "limited", resetsAt: 1_800_000_000 } as const;

// Feeds each change to the rule and then to the board, as the app does, and keeps what alerted.
function alertsOver(changes: Change[], view = AWAY) {
  let board: Board = EMPTY;
  const alerts = [];
  for (const change of changes) {
    alerts.push(alertFor(board, 1, change, view));
    board =
      change.state === "ended"
        ? cleared(board, 1)
        : reported(board, 1, change, view.shown);
  }
  return alerts.filter((a) => a !== undefined);
}

test("a turn that ends in a hidden tab alerts once", () => {
  expect(alertsOver([WORKING, IDLE, IDLE])).toEqual([{ kind: "finished" }]);
});

test("a turn that ends in the shown tab alerts only while the window is unfocused", () => {
  const looking = { shown: 1, focused: true, alertsOn: true };
  expect(alertsOver([WORKING, IDLE], looking)).toEqual([]);

  expect(alertsOver([WORKING, IDLE], { ...looking, focused: false })).toEqual([
    { kind: "finished" },
  ]);
});

test("a hidden tab alerts in an unfocused window too", () => {
  expect(alertsOver([WORKING, IDLE], { ...AWAY, focused: false })).toEqual([
    { kind: "finished" },
  ]);
});

test("nothing alerts with alerts off", () => {
  const off = { ...AWAY, alertsOn: false };
  expect(alertsOver([WORKING, IDLE, WORKING, LIMIT], off)).toEqual([]);
  expect(alertsOver([WORKING, { state: "ended" }], off)).toEqual([]);
});

test("a limit alerts once with its reset time, and not again as finished", () => {
  expect(alertsOver([WORKING, LIMIT, LIMIT, IDLE])).toEqual([
    { kind: "limited", resetsAt: 1_800_000_000 },
  ]);
});

test("each new turn can alert again", () => {
  expect(alertsOver([WORKING, IDLE, WORKING, IDLE])).toEqual([
    { kind: "finished" },
    { kind: "finished" },
  ]);
});

test("a chat that ends alerts only if it was working", () => {
  expect(alertsOver([WORKING, { state: "ended" }])).toEqual([
    { kind: "ended" },
  ]);
  expect(alertsOver([WORKING, IDLE, { state: "ended" }])).toEqual([
    { kind: "finished" },
  ]);
  expect(alertsOver([{ state: "ended" }])).toEqual([]);
});

test("a restored tab's first report alerts nothing, a limit included", () => {
  expect(alertsOver([IDLE])).toEqual([]);
  expect(alertsOver([LIMIT])).toEqual([]);
  expect(alertsOver([WORKING, WORKING])).toEqual([]);
});
