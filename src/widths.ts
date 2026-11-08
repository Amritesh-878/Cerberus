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

export const PANEL_DEFAULT = 300;
export const PANEL_MIN = 240;
const PANEL_MAX = 640;

/** At most two fifths of the window, so the sidebar and the terminal keep room beside it. */
export function panelMax(windowWidth: number): number {
  return Math.round(
    Math.max(PANEL_MIN, Math.min(PANEL_MAX, (windowWidth * 2) / 5)),
  );
}

export function clampPanel(width: number, windowWidth: number): number {
  return Math.min(
    panelMax(windowWidth),
    Math.max(PANEL_MIN, Math.round(width)),
  );
}

/** The left chat's share of a split, in thousandths of the chat area. */
export const SPLIT_DEFAULT = 500;
export const SPLIT_MIN = 200;
export const SPLIT_MAX = 800;

export function clampSplit(share: number): number {
  return Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, Math.round(share)));
}
