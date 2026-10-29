import { describe, expect, test } from "vitest";

import {
  type KeyChord,
  type PastedData,
  pasteInput,
  shortcutFor,
  terminalHandlesKey,
} from "./keys";

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

function pasted(text: string, itemTypes: string[]): PastedData {
  return {
    getData: (format) => (format === "text/plain" ? text : ""),
    items: itemTypes.map((type) => ({
      kind: type.includes("/") ? "file" : "string",
      type,
    })),
  };
}

describe("pasteInput", () => {
  test("an image with no text sends Alt+V to a program that pastes images", () => {
    expect(pasteInput(pasted("", ["image/png"]), true)).toBe("v");
  });

  test.each([
    [
      "text, in a program that pastes images",
      pasted("hello", ["text/plain"]),
      true,
    ],
    ["text beside an image", pasted("hello", ["image/png"]), true],
    ["an image in a shell", pasted("", ["image/png"]), false],
    ["a non-image file", pasted("", ["application/pdf"]), true],
    ["an empty clipboard", pasted("", []), true],
  ])("leaves %s to xterm", (_name, data, pastesImages) => {
    expect(pasteInput(data, pastesImages)).toBeUndefined();
  });
});

describe("shortcutFor", () => {
  test.each([
    ["Ctrl+Tab", chord("Tab", { ctrlKey: true }), "nextTab"],
    [
      "Ctrl+Shift+Tab",
      chord("Tab", { ctrlKey: true, shiftKey: true }),
      "previousTab",
    ],
    ["Ctrl+Shift+T", chord("T", { ctrlKey: true, shiftKey: true }), "newTab"],
    ["Ctrl+Shift+W", chord("W", { ctrlKey: true, shiftKey: true }), "closeTab"],
  ])("maps %s", (_name, key, shortcut) => {
    expect(shortcutFor(key)).toBe(shortcut);
  });

  test.each([
    ["Ctrl+T, Claude Code's task list", chord("t", { ctrlKey: true })],
    ["Ctrl+W, delete a word", chord("w", { ctrlKey: true })],
    ["Shift+Tab, Claude Code's mode switch", chord("Tab", { shiftKey: true })],
    ["Ctrl+C", chord("c", { ctrlKey: true })],
    ["Alt+V", chord("v", { altKey: true })],
    ["Ctrl+Alt+Tab", chord("Tab", { ctrlKey: true, altKey: true })],
  ])("leaves %s to the terminal", (_name, key) => {
    expect(shortcutFor(key)).toBeUndefined();
  });
});
