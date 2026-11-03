import { describe, expect, test } from "vitest";

import {
  isReloadKey,
  type KeyChord,
  keyHelp,
  type PastedData,
  type Platform,
  pasteInput,
  platformOf,
  shortcutFor,
  terminalAction,
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
    expect(terminalHandlesKey(key, "other")).toBe(false);
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
    expect(terminalHandlesKey(key, "other")).toBe(true);
  });

  test.each([
    ["Cmd+V, the menu's paste", chord("v", { metaKey: true })],
    ["Cmd+C, the menu's copy", chord("c", { metaKey: true })],
    ["Cmd+T, a Cerberus key", chord("t", { metaKey: true })],
  ])("keeps %s from the program on macOS", (_name, key) => {
    expect(terminalHandlesKey(key, "mac")).toBe(false);
  });

  test.each([
    ["Ctrl+V, Claude Code's image paste", chord("v", { ctrlKey: true })],
    ["Ctrl+C", chord("c", { ctrlKey: true })],
    ["Ctrl+T, Claude Code's task list", chord("t", { ctrlKey: true })],
    ["Option+B, a word back", chord("b", { altKey: true })],
  ])("passes %s to the program on macOS", (_name, key) => {
    expect(terminalHandlesKey(key, "mac")).toBe(true);
  });
});

test("the platform comes from the user agent", () => {
  expect(
    platformOf(
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
    ),
  ).toBe("mac");
  expect(
    platformOf("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/130.0"),
  ).toBe("other");
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
    expect(shortcutFor(key, "other")).toBe(shortcut);
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
    ["Cmd+T", chord("t", { metaKey: true })],
  ])("leaves %s to the terminal", (_name, key) => {
    expect(shortcutFor(key, "other")).toBeUndefined();
  });
});

describe("shortcutFor on macOS", () => {
  test.each([
    ["Cmd+T", chord("t", { metaKey: true }), "newTab"],
    ["Cmd+W", chord("w", { metaKey: true }), "closeTab"],
    [
      "Cmd+Shift+A",
      chord("a", { metaKey: true, shiftKey: true }),
      "nextWaiting",
    ],
    [
      "Cmd+Shift+], typed as }",
      chord("}", { code: "BracketRight", metaKey: true, shiftKey: true }),
      "nextTab",
    ],
    [
      "Cmd+Shift+[, typed as {",
      chord("{", { code: "BracketLeft", metaKey: true, shiftKey: true }),
      "previousTab",
    ],
    ["Ctrl+Tab", chord("Tab", { ctrlKey: true }), "nextTab"],
    [
      "Ctrl+Shift+Tab",
      chord("Tab", { ctrlKey: true, shiftKey: true }),
      "previousTab",
    ],
  ])("maps %s", (_name, key, shortcut) => {
    expect(shortcutFor(key, "mac")).toBe(shortcut);
  });

  test.each([
    ["Ctrl+T, Claude Code's task list", chord("t", { ctrlKey: true })],
    ["Ctrl+W, delete a word", chord("w", { ctrlKey: true })],
    ["Ctrl+Shift+T", chord("T", { ctrlKey: true, shiftKey: true })],
    ["Cmd+C, the menu's copy", chord("c", { metaKey: true })],
    ["Cmd+Option+T", chord("t", { metaKey: true, altKey: true })],
  ])("leaves %s alone", (_name, key) => {
    expect(shortcutFor(key, "mac")).toBeUndefined();
  });
});

test.each<[Platform, string]>([
  ["other", "Ctrl+Shift+T"],
  ["mac", "Cmd+T"],
])(
  "every chord the %s start page lists is one Cerberus answers to",
  (platform, newTab) => {
    const help = keyHelp(platform);
    const listed = help.flatMap((h) => h.keys.split(" / "));
    const chords = listed.filter((keys) => keys.includes("+"));

    for (const keys of chords) {
      const mods = keys.split("+");
      const key = mods.pop() ?? "";
      const pressed = chord(key, {
        ctrlKey: mods.includes("Ctrl"),
        shiftKey: mods.includes("Shift"),
        metaKey: mods.includes("Cmd"),
      });
      expect(shortcutFor(pressed, platform), keys).toBeDefined();
    }
    expect(chords).toHaveLength(5);
    expect(listed).toContain("F2");
    expect(help.find((h) => h.does === "New tab")?.keys).toBe(newTab);
  },
);

describe("terminalAction on macOS", () => {
  test.each([
    ["Cmd+Left, start of line", chord("ArrowLeft", { metaKey: true }), "\x01"],
    ["Cmd+Right, end of line", chord("ArrowRight", { metaKey: true }), "\x05"],
    [
      "Cmd+Backspace, delete to start of line",
      chord("Backspace", { metaKey: true }),
      "\x15",
    ],
    ["Option+Left, a word back", chord("ArrowLeft", { altKey: true }), "\x1bb"],
    [
      "Option+Right, a word forward",
      chord("ArrowRight", { altKey: true }),
      "\x1bf",
    ],
    [
      "Option+Backspace, delete a word",
      chord("Backspace", { altKey: true }),
      "\x1b\x7f",
    ],
  ])("%s sends its bytes", (_name, key, bytes) => {
    expect(terminalAction(key, "mac")).toEqual({ kind: "send", bytes });
  });

  test("Cmd+K clears the screen and scrollback, Cmd+A selects all", () => {
    expect(terminalAction(chord("k", { metaKey: true }), "mac")).toEqual({
      kind: "clear",
    });
    expect(terminalAction(chord("a", { metaKey: true }), "mac")).toEqual({
      kind: "selectAll",
    });
  });

  test.each([
    ["Cmd+T, new tab", chord("t", { metaKey: true })],
    ["Cmd+W, close tab", chord("w", { metaKey: true })],
    [
      "Cmd+Shift+], next tab",
      chord("}", { code: "BracketRight", metaKey: true, shiftKey: true }),
    ],
    [
      "Cmd+Shift+[, previous tab",
      chord("{", { code: "BracketLeft", metaKey: true, shiftKey: true }),
    ],
    [
      "Cmd+Shift+A, next waiting",
      chord("a", { metaKey: true, shiftKey: true }),
    ],
    ["Cmd+C, the menu's copy", chord("c", { metaKey: true })],
    ["Cmd+V, the menu's paste", chord("v", { metaKey: true })],
    ["Ctrl+C", chord("c", { ctrlKey: true })],
    ["Ctrl+T, Claude Code's task list", chord("t", { ctrlKey: true })],
    ["Shift+Tab, Claude Code's mode switch", chord("Tab", { shiftKey: true })],
    ["Ctrl+A, start of line", chord("a", { ctrlKey: true })],
    ["Option+B, already a word back", chord("b", { altKey: true })],
    ["a plain Left", chord("ArrowLeft")],
    ["Cmd+Shift+Left", chord("ArrowLeft", { metaKey: true, shiftKey: true })],
    ["Cmd+Option+Left", chord("ArrowLeft", { metaKey: true, altKey: true })],
  ])("leaves %s as it was", (_name, key) => {
    expect(terminalAction(key, "mac")).toBeUndefined();
  });

  test("Cmd keys the terminal does not remap still never reach xterm", () => {
    expect(terminalHandlesKey(chord("k", { metaKey: true }), "mac")).toBe(
      false,
    );
    expect(shortcutFor(chord("ArrowLeft", { metaKey: true }), "mac")).toBe(
      undefined,
    );
    expect(shortcutFor(chord("k", { metaKey: true }), "mac")).toBeUndefined();
  });
});

test.each([
  ["Ctrl+Left", chord("ArrowLeft", { ctrlKey: true })],
  ["Alt+Left", chord("ArrowLeft", { altKey: true })],
  ["Alt+Backspace", chord("Backspace", { altKey: true })],
  ["Ctrl+Backspace", chord("Backspace", { ctrlKey: true })],
  ["Ctrl+K", chord("k", { ctrlKey: true })],
  ["Ctrl+A", chord("a", { ctrlKey: true })],
  ["Win+Left", chord("ArrowLeft", { metaKey: true })],
])("Windows leaves %s to xterm", (_name, key) => {
  expect(terminalAction(key, "other")).toBeUndefined();
});

describe("isReloadKey", () => {
  test.each([
    ["F5", chord("F5")],
    ["Ctrl+F5", chord("F5", { ctrlKey: true })],
    ["Shift+F5", chord("F5", { shiftKey: true })],
    ["Ctrl+R", chord("r", { ctrlKey: true })],
    ["Ctrl+Shift+R", chord("R", { ctrlKey: true, shiftKey: true })],
  ])("%s reloads on Windows", (_name, key) => {
    expect(isReloadKey(key, "other")).toBe(true);
  });

  test.each([
    ["Cmd+R", chord("r", { metaKey: true })],
    ["Cmd+Shift+R", chord("R", { metaKey: true, shiftKey: true })],
    ["F5", chord("F5")],
  ])("%s reloads on macOS", (_name, key) => {
    expect(isReloadKey(key, "mac")).toBe(true);
  });

  test.each<[string, KeyChord, Platform]>([
    ["a plain r", chord("r"), "other"],
    ["Alt+R", chord("r", { altKey: true }), "other"],
    ["Ctrl+T", chord("t", { ctrlKey: true }), "other"],
    [
      "Ctrl+R, the shell's reverse search",
      chord("r", { ctrlKey: true }),
      "mac",
    ],
    ["Cmd+Option+R", chord("r", { metaKey: true, altKey: true }), "mac"],
  ])("%s is not a reload", (_name, key, platform) => {
    expect(isReloadKey(key, platform)).toBe(false);
  });
});
