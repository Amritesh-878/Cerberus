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
  | "nextTab"
  | "previousTab"
  | "newTab"
  | "closeTab"
  | "nextWaiting"
  | "panel"
  | "findChat"
  | "shortcuts";

type Chords = readonly (readonly [Shortcut, string])[];

// Cerberus's own keys, chosen to miss Claude Code's (Ctrl+T, Ctrl+C, Ctrl+V, Alt+V, Shift+Tab) and WebView2's Ctrl+Shift+J.
const OTHER_CHORDS: Chords = [
  ["nextTab", "Ctrl+Tab"],
  ["previousTab", "Ctrl+Shift+Tab"],
  ["newTab", "Ctrl+Shift+T"],
  ["nextWaiting", "Ctrl+Shift+A"],
  ["closeTab", "Ctrl+Shift+W"],
  // Ctrl+B is Claude Code's background-task key, so the panel takes it only with Shift.
  ["panel", "Ctrl+Shift+B"],
  // Ctrl+F stays with the terminal; Find a chat takes it only with Shift.
  ["findChat", "Ctrl+Shift+F"],
  // Ctrl+/ stays with the terminal, where editors and shells use it.
  ["shortcuts", "Ctrl+Shift+/"],
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
  ["panel", "Cmd+Shift+B"],
  ["findChat", "Cmd+Shift+F"],
  ["shortcuts", "Cmd+/"],
];

function chordsFor(platform: Platform): Chords {
  return platform === "mac" ? MAC_CHORDS : OTHER_CHORDS;
}

/** Renames the sidebar row or project that has focus. */
export const RENAME_KEY = "F2";

// Shift turns `]` into `}` and `/` into `?` on a US layout, so these match by physical key.
const CODE_KEYS: Readonly<Record<string, string>> = {
  BracketLeft: "[",
  BracketRight: "]",
  Slash: "/",
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
const MAC_TERMINAL_KEYS: readonly (readonly [
  string,
  TerminalAction,
  string,
])[] = [
  ["Cmd+ArrowLeft", { kind: "send", bytes: "\x01" }, "Start of line"],
  ["Cmd+ArrowRight", { kind: "send", bytes: "\x05" }, "End of line"],
  ["Cmd+Backspace", { kind: "send", bytes: "\x15" }, "Delete to start of line"],
  ["Option+ArrowLeft", { kind: "send", bytes: "\x1bb" }, "Word back"],
  ["Option+ArrowRight", { kind: "send", bytes: "\x1bf" }, "Word forward"],
  ["Option+Backspace", { kind: "send", bytes: "\x1b\x7f" }, "Delete word back"],
  ["Cmd+K", { kind: "clear" }, "Clear the screen"],
  ["Cmd+A", { kind: "selectAll" }, "Select all"],
];

export type TabKind = "claude" | "shell";

const NEWLINE_KEY = "Shift+Enter";
// Claude Code reads this CSI u form as a newline, while xterm's plain Enter would send the message.
const CLAUDE_NEWLINE: TerminalAction = { kind: "send", bytes: "\x1b[13;2u" };

/** Shift+Enter makes a newline in Claude Code; otherwise only macOS remaps keys. */
export function terminalAction(
  chord: KeyChord,
  kind: TabKind,
  platform: Platform = PLATFORM,
): TerminalAction | undefined {
  if (kind === "claude" && matches(chord, NEWLINE_KEY)) return CLAUDE_NEWLINE;
  if (platform !== "mac") return undefined;
  return MAC_TERMINAL_KEYS.find(([keys]) => matches(chord, keys))?.[1];
}

/** Keys that would reload the page, which ends every tab's program. */
export function isReloadKey(
  chord: KeyChord,
  platform: Platform = PLATFORM,
): boolean {
  if (chord.key === "F5") return true;
  const key = chord.key.toLowerCase();
  const command = platform === "mac" ? chord.metaKey : chord.ctrlKey;
  const other = platform === "mac" ? chord.ctrlKey : chord.metaKey;
  return key === "r" && command && !other && !chord.altKey;
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
    { keys: keys("shortcuts"), does: "Keyboard shortcuts" },
  ];
}

export interface SheetSection {
  title: string;
  rows: readonly { does: string; keys: readonly string[] }[];
}

const SHEET_ROWS: readonly (readonly [Shortcut, string])[] = [
  ["nextTab", "Next tab"],
  ["previousTab", "Previous tab"],
  ["newTab", "New tab"],
  ["closeTab", "Close tab"],
  ["nextWaiting", "Next chat waiting on you"],
  ["panel", "Show or hide the right panel"],
  ["findChat", "Find a chat on any account"],
  ["shortcuts", "Show or hide this sheet"],
];

/** Every key Cerberus answers to, alternates included, read from the tables the handlers use. */
export function shortcutSheet(
  platform: Platform = PLATFORM,
): readonly SheetSection[] {
  const chords = chordsFor(platform);
  const app: SheetSection = {
    title: "Tabs",
    rows: [
      ...SHEET_ROWS.map(([shortcut, does]) => ({
        does,
        keys: chords.filter(([s]) => s === shortcut).map(([, keys]) => keys),
      })),
      { does: "Rename the focused tab or project", keys: [RENAME_KEY] },
    ],
  };
  const claude: SheetSection = {
    title: "Claude Code",
    rows: [{ does: "New line in a message", keys: [NEWLINE_KEY] }],
  };
  if (platform !== "mac") return [app, claude];
  const terminal: SheetSection = {
    title: "Terminal",
    rows: MAC_TERMINAL_KEYS.map(([keys, , does]) => ({ does, keys: [keys] })),
  };
  return [app, claude, terminal];
}
