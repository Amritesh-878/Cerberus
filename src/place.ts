export interface Anchor {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export interface Viewport {
  width: number;
  height: number;
}

/** `below` drops the box under its anchor, as a menu at a click; `beside` opens it to the right, as from a button. */
export type Side = "below" | "beside";

const GAP = 4;
const MARGIN = 8;
const UNANCHORED_TOP = 64;

export function placeBox(
  anchor: Anchor | undefined,
  box: { width: number; height: number },
  viewport: Viewport,
  side: Side = "below",
): { left: number; top: number } {
  const maxLeft = viewport.width - box.width - MARGIN;
  const maxTop = viewport.height - box.height - MARGIN;
  if (!anchor) {
    return {
      left: Math.max(MARGIN, (viewport.width - box.width) / 2),
      top: Math.max(MARGIN, Math.min(UNANCHORED_TOP, maxTop)),
    };
  }
  if (side === "beside") {
    const right = anchor.right + GAP;
    const left = anchor.left - GAP - box.width;
    const x = right <= maxLeft ? right : left >= MARGIN ? left : maxLeft;
    return {
      left: Math.max(MARGIN, x),
      top: Math.max(MARGIN, Math.min(anchor.top, maxTop)),
    };
  }
  const left = Math.max(MARGIN, Math.min(anchor.left, maxLeft));
  const below = anchor.bottom + GAP;
  if (below <= maxTop) return { left, top: below };
  const above = anchor.top - GAP - box.height;
  if (above >= MARGIN) return { left, top: above };
  return { left, top: Math.max(MARGIN, maxTop) };
}

/** A trigger hidden in a closed drawer or scrolled out of the window cannot anchor anything. */
export function onScreen(
  rect: Anchor & { width: number; height: number },
  viewport: Viewport,
): boolean {
  return (
    rect.width > 0 &&
    rect.height > 0 &&
    rect.left >= 0 &&
    rect.top >= 0 &&
    rect.right <= viewport.width &&
    rect.bottom <= viewport.height
  );
}
