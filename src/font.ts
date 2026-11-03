import type { Platform } from "./keys";

// xterm draws bold in the same family, so the stack must start with a font every Mac has.
const MAC_FONT = 'ui-monospace, "SF Mono", Menlo, Monaco, monospace';
const OTHER_FONT = '"Cascadia Mono", Consolas, monospace';

export function terminalFont(platform: Platform): string {
  return platform === "mac" ? MAC_FONT : OTHER_FONT;
}
