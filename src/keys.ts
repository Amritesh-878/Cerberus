export interface KeyChord {
  key: string;
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

/** Text pastes go to the webview's paste event; Alt+V stays input, as Claude Code's image paste. */
export function terminalHandlesKey(chord: KeyChord): boolean {
  return !isTextPaste(chord);
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

// Cerberus's own keys, chosen to miss Claude Code's (Ctrl+T, Ctrl+C, Ctrl+V, Alt+V, Shift+Tab) and WebView2's Ctrl+Shift+J.
const CHORDS: readonly (readonly [Shortcut, string])[] = [
  ["nextTab", "Ctrl+Tab"],
  ["previousTab", "Ctrl+Shift+Tab"],
  ["newTab", "Ctrl+Shift+T"],
  ["nextWaiting", "Ctrl+Shift+A"],
  ["closeTab", "Ctrl+Shift+W"],
];

/** Renames the sidebar row or project that has focus. */
export const RENAME_KEY = "F2";

function matches(chord: KeyChord, keys: string): boolean {
  const mods = keys.split("+");
  const key = mods.pop()?.toLowerCase();
  return (
    !chord.altKey &&
    !chord.metaKey &&
    chord.ctrlKey === mods.includes("Ctrl") &&
    chord.shiftKey === mods.includes("Shift") &&
    chord.key.toLowerCase() === key
  );
}

export function shortcutFor(chord: KeyChord): Shortcut | undefined {
  return CHORDS.find(([, keys]) => matches(chord, keys))?.[0];
}

function keysOf(shortcut: Shortcut): string {
  return CHORDS.find(([s]) => s === shortcut)?.[1] ?? "";
}

/** What the start page lists, built from the same keys `shortcutFor` answers to. */
export const KEY_HELP: readonly { keys: string; does: string }[] = [
  {
    keys: `${keysOf("nextTab")} / ${keysOf("previousTab")}`,
    does: "Switch tabs",
  },
  { keys: keysOf("newTab"), does: "New tab" },
  { keys: keysOf("nextWaiting"), does: "Next chat waiting on you" },
  { keys: RENAME_KEY, does: "Rename" },
  { keys: keysOf("closeTab"), does: "Close tab" },
];
