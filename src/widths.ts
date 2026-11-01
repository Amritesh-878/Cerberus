export const SIDEBAR_DEFAULT = 248;
const SIDEBAR_MIN = 200;
const SIDEBAR_MAX = 560;

/** At most half the window, so the terminal keeps room however wide the sidebar is dragged. */
export function clampSidebar(width: number, windowWidth: number): number {
  const most = Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, windowWidth / 2));
  return Math.round(Math.min(most, Math.max(SIDEBAR_MIN, width)));
}
