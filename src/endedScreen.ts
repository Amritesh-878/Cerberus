const NOTE = "\x1b[2m[the chat ended; this tab is now a shell]\x1b[0m";
// Clears the screen in place, so the chat's last frame goes and earlier scrollback stays.
const CLEAR = "\x1b[H\x1b[2J";

/** What replaces a chat's last frame once it ends: its resume line, then the note, at the top. */
export function endedScreen(visible: readonly string[]): string {
  const at = visible.findIndex((line) =>
    line.trimStart().startsWith("Resume this session with"),
  );
  const kept: string[] = [];
  if (at >= 0) {
    // A narrow window wraps the command onto the following lines, up to a blank one.
    for (const line of visible.slice(at)) {
      if (line.trim() === "") break;
      kept.push(line.trimEnd());
    }
  }
  return `${CLEAR}${kept.map((line) => `${line}\r\n`).join("")}${NOTE}\r\n`;
}
