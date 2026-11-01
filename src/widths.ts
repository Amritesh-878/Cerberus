export const SIDEBAR_DEFAULT = 248;
export const SIDEBAR_MIN = 200;
const SIDEBAR_MAX = 560;

/** At most half the window, so the terminal keeps room however wide the sidebar is dragged. */
export function sidebarMax(windowWidth: number): number {
  return Math.round(
    Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, windowWidth / 2)),
  );
}

export function clampSidebar(width: number, windowWidth: number): number {
  return Math.min(
    sidebarMax(windowWidth),
    Math.max(SIDEBAR_MIN, Math.round(width)),
  );
}
