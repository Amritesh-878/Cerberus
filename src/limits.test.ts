import { expect, test } from "vitest";

import { type LimitHit, LimitQueue } from "./limits";

const NOW = 1_900_000_000;
const AHEAD = NOW + 3600;

function hit(change: Partial<LimitHit> = {}): LimitHit {
  return {
    tab: 2,
    account: 1,
    resetsAt: AHEAD,
    offers: [0],
    source: "meter",
    ...change,
  };
}

test("a clock record without a time and a meter's time still ahead are one window, either way round", () => {
  const clockFirst = new LimitQueue();
  expect(clockFirst.hit(hit({ resetsAt: null, source: "clock" }), NOW)).toBe(
    true,
  );
  clockFirst.done();
  expect(clockFirst.hit(hit(), NOW)).toBe(false);
  expect(clockFirst.first()).toBeUndefined();

  const meterFirst = new LimitQueue();
  meterFirst.hit(hit(), NOW);
  meterFirst.done();
  expect(meterFirst.hit(hit({ resetsAt: null, source: "clock" }), NOW)).toBe(
    false,
  );
  expect(meterFirst.first()).toBeUndefined();
});

test("a clock record without a time after a window that has reset is a new window", () => {
  const queue = new LimitQueue();
  queue.hit(hit({ resetsAt: NOW - 60 }), NOW);
  queue.done();

  expect(queue.hit(hit({ resetsAt: null, source: "clock" }), NOW)).toBe(true);
});

test("a later report of the same window refreshes the waiting popup's offers without queueing another", () => {
  const queue = new LimitQueue();
  queue.hit(hit({ tab: 5 }), NOW);
  queue.hit(hit({ offers: [0, 3] }), NOW);

  expect(queue.hit(hit({ offers: [3] }), NOW)).toBe(true);
  expect(queue.hit(hit({ offers: [3] }), NOW)).toBe(false);

  queue.done();
  expect(queue.first()).toEqual(hit({ offers: [3] }));
  queue.done();
  expect(queue.first()).toBeUndefined();
});

test("a chat leaving limited drops its clock popups and keeps its meter's", () => {
  const queue = new LimitQueue();
  queue.hit(hit({ source: "clock" }), NOW);
  queue.hit(hit({ tab: 3 }), NOW);
  queue.hit(hit({ resetsAt: AHEAD + 18_000 }), NOW);

  queue.cleared(2);

  expect(queue.first()).toEqual(hit({ tab: 3 }));
  queue.done();
  expect(queue.first()).toEqual(hit({ resetsAt: AHEAD + 18_000 }));
});

test("after the chat leaves limited, a later record without a time is told again", () => {
  const queue = new LimitQueue();
  queue.hit(hit({ resetsAt: null, source: "clock" }), NOW);
  queue.done();
  expect(queue.hit(hit({ resetsAt: null, source: "clock" }), NOW)).toBe(false);

  queue.cleared(2);

  expect(queue.hit(hit({ resetsAt: null, source: "clock" }), NOW)).toBe(true);
});
