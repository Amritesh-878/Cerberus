export interface KeyChord {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** Ctrl+V, Ctrl+Shift+V and Shift+Insert, which paste clipboard text. */
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
