/** Where a tab dragged out of the sidebar is, so the chat area can offer to split with it. */
export interface TabDragEvents {
  move: (x: number, y: number) => void;
  drop: (x: number, y: number) => void;
  cancel: () => void;
}

// Past this many pixels a press on a tab is a drag, not a click.
const DRAG_START_PX = 6;

type Press = PointerEvent & { currentTarget: HTMLElement };

/** Pointer handlers for a tab's row, with pointer events since the webview's own drag and drop is the window's on Windows; `events` absent, the row doesn't drag. */
export function tabDrag() {
  let start: { pointer: number; x: number; y: number } | undefined;
  let dragging = false;
  // A drop ends with a click on the row, which must not select the dragged tab.
  let swallowClick = false;
  const end = (event: Press) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    start = undefined;
    dragging = false;
  };
  return {
    onPointerDown: (event: Press) => {
      swallowClick = false;
      // A finger on the list scrolls it; dragging a tab in is for a mouse or pen.
      if (event.pointerType === "touch") return;
      if (event.button !== 0 || start !== undefined) return;
      start = { pointer: event.pointerId, x: event.clientX, y: event.clientY };
      // Captured at once, so a quick flick that leaves the row in one frame still drags.
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    onPointerMove: (event: Press, events: TabDragEvents | undefined) => {
      if (event.pointerId !== start?.pointer) return;
      if (!dragging) {
        const far =
          Math.hypot(event.clientX - start.x, event.clientY - start.y) >=
          DRAG_START_PX;
        if (!far || !events) return;
        dragging = true;
      }
      events?.move(event.clientX, event.clientY);
    },
    onPointerUp: (event: Press, events: TabDragEvents | undefined) => {
      if (event.pointerId !== start?.pointer) return;
      if (dragging) {
        swallowClick = true;
        events?.drop(event.clientX, event.clientY);
      }
      end(event);
    },
    onPointerCancel: (event: Press, events: TabDragEvents | undefined) => {
      if (event.pointerId !== start?.pointer) return;
      if (dragging) events?.cancel();
      end(event);
    },
    /** True once after a drop, for the click that follows it. */
    swallowClick: () => {
      const swallow = swallowClick;
      swallowClick = false;
      return swallow;
    },
  };
}
