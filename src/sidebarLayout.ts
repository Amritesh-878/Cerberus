/** The rail's width when the sidebar is collapsed. */
export const RAIL_WIDTH = 48;
/** Below this the sidebar is a drawer over the chat, as before. */
export const DRAWER_BELOW = 720;
/** Below this the sidebar folds to the rail by itself, without changing the saved choice. */
export const RAIL_BELOW = 960;
/** About 60 columns of 14px monospace plus the terminal's padding. */
export const CHAT_MIN = 520;

export type SidebarMode = "drawer" | "rail" | "open";

/** `narrowOpen` is the user opening the sidebar while the window is too narrow for it to stay open. */
export function sidebarMode(
  windowWidth: number,
  collapsed: boolean,
  narrowOpen: boolean,
): SidebarMode {
  if (windowWidth <= DRAWER_BELOW) return "drawer";
  if (windowWidth < RAIL_BELOW) return narrowOpen ? "open" : "rail";
  return collapsed ? "rail" : "open";
}

/** Whether the panel goes over the chat, which beside it would drop under `CHAT_MIN`. */
export function panelOverlays(
  windowWidth: number,
  sidebarWidth: number,
  panelWidth: number,
): boolean {
  return windowWidth - sidebarWidth - panelWidth < CHAT_MIN;
}

// Whole characters, so an emoji or an accented letter is never cut in half.
function graphemes(text: string): string[] {
  return Array.from(new Intl.Segmenter().segment(text), (g) => g.segment);
}

/** The letters a project's rail icon shows: its first two words' initials, or its first two letters. */
export function initials(name: string): string {
  const words = name
    .trim()
    .split(/[\s_-]+/u)
    .filter(Boolean);
  const letters =
    words.length >= 2
      ? words.slice(0, 2).map((word) => graphemes(word)[0] ?? "")
      : graphemes(words[0] ?? "?").slice(0, 2);
  return letters.join("").toUpperCase();
}
