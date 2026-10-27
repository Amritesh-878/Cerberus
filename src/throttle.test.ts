import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { throttle } from "./throttle";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

/** Calls every `step` ms for `duration` ms and returns the times `fn` ran. */
function drag(step: number, duration: number): number[] {
  const runs: number[] = [];
  const throttled = throttle(() => runs.push(Date.now()), 30);
  const start = Date.now();
  for (let t = 0; t <= duration; t += step) {
    throttled.call();
    vi.advanceTimersByTime(step);
  }
  vi.advanceTimersByTime(100);
  return runs.map((at) => at - start);
}

test("the first call runs at once", () => {
  expect(drag(5, 0)).toEqual([0]);
});

test("a steady stream of calls runs once per window, not once at the end", () => {
  const runs = drag(5, 100);

  expect(runs).toEqual([0, 30, 60, 90, 120]);
});

test("the last call always gets a run after it", () => {
  const runs = drag(5, 40);

  expect(runs.at(-1)).toBeGreaterThanOrEqual(40);
});

test("cancel drops the pending run", () => {
  const fn = vi.fn();
  const throttled = throttle(fn, 30);

  throttled.call();
  throttled.call();
  throttled.cancel();
  vi.advanceTimersByTime(100);

  expect(fn).toHaveBeenCalledTimes(1);
});
