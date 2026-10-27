import { describe, expect, test } from "vitest";

import { type KeyChord, terminalHandlesKey } from "./keys";

function chord(key: string, mods: Partial<KeyChord> = {}): KeyChord {
  return {
    key,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  };
}

describe("terminalHandlesKey", () => {
  test.each([
    ["Ctrl+V", chord("v", { ctrlKey: true })],
    ["Ctrl+Shift+V", chord("V", { ctrlKey: true, shiftKey: true })],
    ["Shift+Insert", chord("Insert", { shiftKey: true })],
  ])("hands %s to the webview as a text paste", (_name, key) => {
    expect(terminalHandlesKey(key)).toBe(false);
  });

  test.each([
    ["Alt+V, Claude Code's image paste", chord("v", { altKey: true })],
    ["Ctrl+Alt+V", chord("v", { ctrlKey: true, altKey: true })],
    ["Ctrl+C", chord("c", { ctrlKey: true })],
    ["a plain v", chord("v")],
    ["Insert", chord("Insert")],
  ])("passes %s to the program", (_name, key) => {
    expect(terminalHandlesKey(key)).toBe(true);
  });
});
