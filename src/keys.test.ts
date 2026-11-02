import { describe, expect, test } from "vitest";

import {
  KEY_HELP,
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
    ["Ctrl+A, start of line", chord("a", { ctrlKey: true })],
    [
      "Ctrl+Shift+C, Claude Code's copy",
      chord("C", { ctrlKey: true, shiftKey: true }),
    ],
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
  test("an image with no text sends the program's image-paste key", () => {
    expect(pasteInput(pasted("", ["image/png"]), "\x1bv")).toBe("\x1bv");
    expect(pasteInput(pasted("", ["image/png"]), "\x16")).toBe("\x16");
  });

  test.each([
    [
      "text, in a program that pastes images",
      pasted("hello", ["text/plain"]),
      "\x1bv",
    ],
    ["text beside an image", pasted("hello", ["image/png"]), "\x1bv"],
    ["an image in a shell", pasted("", ["image/png"]), null],
    ["a non-image file", pasted("", ["application/pdf"]), "\x1bv"],
    ["an empty clipboard", pasted("", []), "\x1bv"],
  ])("leaves %s to xterm", (_name, data, imagePasteKey) => {
    expect(pasteInput(data, imagePasteKey)).toBeUndefined();
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
    [
      "Ctrl+Shift+A",
      chord("A", { ctrlKey: true, shiftKey: true }),
      "nextWaiting",
    ],
  ])("maps %s", (_name, key, shortcut) => {
    expect(shortcutFor(key)).toBe(shortcut);
  });

  test.each([
    ["Ctrl+T, Claude Code's task list", chord("t", { ctrlKey: true })],
    ["Ctrl+W, delete a word", chord("w", { ctrlKey: true })],
    ["Shift+Tab, Claude Code's mode switch", chord("Tab", { shiftKey: true })],
    ["Ctrl+C", chord("c", { ctrlKey: true })],
    ["Ctrl+A, start of line", chord("a", { ctrlKey: true })],
    [
      "Ctrl+Shift+C, Claude Code's copy",
      chord("C", { ctrlKey: true, shiftKey: true }),
    ],
    ["Alt+V", chord("v", { altKey: true })],
    ["Ctrl+Alt+Tab", chord("Tab", { ctrlKey: true, altKey: true })],
  ])("leaves %s to the terminal", (_name, key) => {
    expect(shortcutFor(key)).toBeUndefined();
  });
});

test("every chord the start page lists is one Cerberus answers to", () => {
  const listed = KEY_HELP.flatMap((help) => help.keys.split(" / "));
  const chords = listed.filter((keys) => keys.includes("+"));

  for (const keys of chords) {
    const mods = keys.split("+");
    const key = mods.pop() ?? "";
    const pressed = chord(key, {
      ctrlKey: mods.includes("Ctrl"),
      shiftKey: mods.includes("Shift"),
    });
    expect(shortcutFor(pressed), keys).toBeDefined();
  }
  expect(chords).toHaveLength(5);
  expect(listed).toContain("F2");
});
