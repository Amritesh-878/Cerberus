import type { Platform } from "./keys";

// xterm draws bold in the same family, so the stack must start with a font every Mac has.
const MAC_FONT = 'ui-monospace, "SF Mono", Menlo, Monaco, monospace';
// Ubuntu ships DejaVu and Ubuntu Mono; Cascadia and Consolas are Windows fonts.
const LINUX_FONT =
  '"DejaVu Sans Mono", "Ubuntu Mono", "Liberation Mono", monospace';
const OTHER_FONT = '"Cascadia Mono", Consolas, monospace';

export function terminalFont(platform: Platform): string {
  if (platform === "mac") return MAC_FONT;
  return platform === "linux" ? LINUX_FONT : OTHER_FONT;
}
