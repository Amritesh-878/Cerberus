export interface Anchor {
  left: number;
  top: number;
  bottom: number;
}

export interface Viewport {
  width: number;
  height: number;
}

const GAP = 4;
const MARGIN = 8;
const UNANCHORED_TOP = 64;

/** Where a box opens by its anchor: below it, above it when below is short, else as low as fits. */
export function placeBox(
  anchor: Anchor | undefined,
  box: { width: number; height: number },
  viewport: Viewport,
): { left: number; top: number } {
  if (!anchor) {
    return {
      left: Math.max(MARGIN, (viewport.width - box.width) / 2),
      top: Math.max(
        MARGIN,
        Math.min(UNANCHORED_TOP, viewport.height - MARGIN - box.height),
      ),
    };
  }
  const left = Math.max(
    MARGIN,
    Math.min(anchor.left, viewport.width - box.width - MARGIN),
  );
  const below = anchor.bottom + GAP;
  if (below + box.height <= viewport.height - MARGIN)
    return { left, top: below };
  const above = anchor.top - GAP - box.height;
  if (above >= MARGIN) return { left, top: above };
  return { left, top: Math.max(MARGIN, viewport.height - MARGIN - box.height) };
}
