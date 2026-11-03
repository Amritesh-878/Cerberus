export interface KeyChord {
  key: string;
  /** The physical key, for chords whose `key` changes with Shift, such as `]`. */
  code?: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export interface PastedData {
  getData: (format: string) => string;
  items: ArrayLike<{ kind: string; type: string }>;
}

function isTextPaste(chord: KeyChord): boolean {
  if (chord.altKey || chord.metaKey) return false;
  const key = chord.key.toLowerCase();
  return (
    (chord.ctrlKey && key === "v") ||
    (chord.shiftKey && !chord.ctrlKey && key === "insert")
  );
}

export type Platform = "mac" | "other";

export function platformOf(userAgent: string): Platform {
  return /Macintosh|Mac OS X/.test(userAgent) ? "mac" : "other";
}

export const PLATFORM: Platform = platformOf(navigator.userAgent);

/** On macOS Cmd keys belong to Cerberus and the menu; elsewhere text pastes go to the webview's paste event. */
export function terminalHandlesKey(
  chord: KeyChord,
  platform: Platform = PLATFORM,
): boolean {
  return platform === "mac" ? !chord.metaKey : !isTextPaste(chord);
}

/** Input to send in place of a paste, or `undefined` to let xterm paste the text. */
export function pasteInput(
  pasted: PastedData,
  imagePasteKey: string | null,
): string | undefined {
  if (imagePasteKey === null || pasted.getData("text/plain") !== "") {
    return undefined;
  }
  const hasImage = Array.from(pasted.items).some(
    (item) => item.kind === "file" && item.type.startsWith("image/"),
  );
  return hasImage ? imagePasteKey : undefined;
}

export type Shortcut =
  "nextTab" | "previousTab" | "newTab" | "closeTab" | "nextWaiting";

type Chords = readonly (readonly [Shortcut, string])[];

// Cerberus's own keys, chosen to miss Claude Code's (Ctrl+T, Ctrl+C, Ctrl+V, Alt+V, Shift+Tab) and WebView2's Ctrl+Shift+J.
const OTHER_CHORDS: Chords = [
  ["nextTab", "Ctrl+Tab"],
  ["previousTab", "Ctrl+Shift+Tab"],
  ["newTab", "Ctrl+Shift+T"],
  ["nextWaiting", "Ctrl+Shift+A"],
  ["closeTab", "Ctrl+Shift+W"],
];

// xterm never encodes Cmd keys on macOS, so Terminal.app's keys take nothing from Claude Code.
const MAC_CHORDS: Chords = [
  ["nextTab", "Cmd+Shift+]"],
  ["previousTab", "Cmd+Shift+["],
  ["nextTab", "Ctrl+Tab"],
  ["previousTab", "Ctrl+Shift+Tab"],
  ["newTab", "Cmd+T"],
  ["nextWaiting", "Cmd+Shift+A"],
  ["closeTab", "Cmd+W"],
];

function chordsFor(platform: Platform): Chords {
  return platform === "mac" ? MAC_CHORDS : OTHER_CHORDS;
}

/** Renames the sidebar row or project that has focus. */
export const RENAME_KEY = "F2";

// Shift turns `]` into `}` on a US layout, so brackets match by physical key.
const CODE_KEYS: Readonly<Record<string, string>> = {
  BracketLeft: "[",
  BracketRight: "]",
};

function matches(chord: KeyChord, keys: string): boolean {
  const mods = keys.split("+");
  const key = mods.pop()?.toLowerCase();
  const pressed = chord.key.toLowerCase();
  return (
    chord.altKey === mods.includes("Option") &&
    chord.metaKey === mods.includes("Cmd") &&
    chord.ctrlKey === mods.includes("Ctrl") &&
    chord.shiftKey === mods.includes("Shift") &&
    (pressed === key || CODE_KEYS[chord.code ?? ""] === key)
  );
}

export function shortcutFor(
  chord: KeyChord,
  platform: Platform = PLATFORM,
): Shortcut | undefined {
  return chordsFor(platform).find(([, keys]) => matches(chord, keys))?.[0];
}

/** What a key pressed in the terminal does in place of xterm's own encoding. */
export type TerminalAction =
  { kind: "send"; bytes: string } | { kind: "clear" } | { kind: "selectAll" };

// Terminal.app's natural text editing, with the bytes Ghostty sends by default on macOS.
const MAC_TERMINAL_KEYS: readonly (readonly [string, TerminalAction])[] = [
  ["Cmd+ArrowLeft", { kind: "send", bytes: "\x01" }],
  ["Cmd+ArrowRight", { kind: "send", bytes: "\x05" }],
  ["Cmd+Backspace", { kind: "send", bytes: "\x15" }],
  ["Option+ArrowLeft", { kind: "send", bytes: "\x1bb" }],
  ["Option+ArrowRight", { kind: "send", bytes: "\x1bf" }],
  ["Option+Backspace", { kind: "send", bytes: "\x1b\x7f" }],
  ["Cmd+K", { kind: "clear" }],
  ["Cmd+A", { kind: "selectAll" }],
];

/** Only macOS remaps keys; Windows keys reach xterm unchanged. */
export function terminalAction(
  chord: KeyChord,
  platform: Platform = PLATFORM,
): TerminalAction | undefined {
  if (platform !== "mac") return undefined;
  return MAC_TERMINAL_KEYS.find(([keys]) => matches(chord, keys))?.[1];
}

function keysOf(shortcut: Shortcut, platform: Platform): string {
  return chordsFor(platform).find(([s]) => s === shortcut)?.[1] ?? "";
}

/** What the start page lists, built from the same keys `shortcutFor` answers to. */
export function keyHelp(
  platform: Platform = PLATFORM,
): readonly { keys: string; does: string }[] {
  const keys = (shortcut: Shortcut) => keysOf(shortcut, platform);
  return [
    {
      keys: `${keys("nextTab")} / ${keys("previousTab")}`,
      does: "Switch tabs",
    },
    { keys: keys("newTab"), does: "New tab" },
    { keys: keys("nextWaiting"), does: "Next chat waiting on you" },
    { keys: RENAME_KEY, does: "Rename" },
    { keys: keys("closeTab"), does: "Close tab" },
  ];
}
