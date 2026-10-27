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

// Alt+V: Claude Code on Windows reads it as "paste the clipboard image" and reads the clipboard itself.
const IMAGE_PASTE_KEY = "\x1bv";

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
  pastesImages: boolean,
): string | undefined {
  if (!pastesImages || pasted.getData("text/plain") !== "") return undefined;
  const hasImage = Array.from(pasted.items).some(
    (item) => item.kind === "file" && item.type.startsWith("image/"),
  );
  return hasImage ? IMAGE_PASTE_KEY : undefined;
}
