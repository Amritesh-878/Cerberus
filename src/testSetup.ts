import { afterEach, expect } from "vitest";

// Solid's warning for a computation with no owner, such as a logical prop first read from a click handler.
const OWNERLESS = "will never be disposed";

const ownerless: string[] = [];
const warn = console.warn.bind(console);
console.warn = (...args: unknown[]) => {
  if (args.map(String).join(" ").includes(OWNERLESS)) {
    // The app's frames only, which name the read that made it.
    const frames = (new Error().stack ?? "")
      .split("\n")
      .filter((line) => line.includes("/src/") && !line.includes("testSetup"));
    ownerless.push(frames.join("\n"));
  }
  warn(...args);
};

// Such a computation outlives what it reads; one that read a closed popup's tab threw and left its backdrop over the window.
afterEach(() => {
  expect(
    ownerless.splice(0),
    "a computation was created with no owner",
  ).toEqual([]);
});
