/** Where a tab dragged out of the sidebar is, so the chat area can offer to split with it. */
export interface TabDragEvents {
  move: (x: number, y: number) => void;
  drop: (x: number, y: number) => void;
  cancel: () => void;
}

// Past this many pixels a press on a tab is a drag, not a click.
const DRAG_START_PX = 6;

type Press = PointerEvent & { currentTarget: HTMLElement };

// Another window taking focus (a snipping tool, Alt+Tab) can swallow the release, so these end a drag too.
const ABANDON_EVENTS = [
  "blur",
  "keydown",
  "visibilitychange",
  "pointermove",
  "mousemove",
  "dragend",
  "drop",
] as const;

/** Pointer handlers for a tab's row, with pointer events since the webview's own drag and drop is the window's on Windows; `events` absent, the row doesn't drag. */
export function tabDrag() {
  let start: { pointer: number; x: number; y: number } | undefined;
  let held: { row: HTMLElement; events: TabDragEvents } | undefined;
  // A drop ends with a click on the row, which must not select the dragged tab.
  let swallowClick = false;
  let unwatch = () => {};

  const end = (row: HTMLElement, pointer: number) => {
    if (row.hasPointerCapture(pointer)) row.releasePointerCapture(pointer);
    start = undefined;
    held = undefined;
    unwatch();
  };
  // Ends a drag whose release never came, so the chat area's drop overlay goes with it.
  const abandon = () => {
    const pointer = start?.pointer;
    const was = held;
    if (pointer === undefined || !was) return;
    end(was.row, pointer);
    was.events.cancel();
  };
  const watch = () => {
    const check = (event: Event) => {
      // A move with no button held means the release happened where this window couldn't see it.
      if (event instanceof MouseEvent && event.buttons !== 0) return;
      if (event.type === "visibilitychange" && !document.hidden) return;
      abandon();
    };
    for (const type of ABANDON_EVENTS) {
      const target = type === "visibilitychange" ? document : window;
      target.addEventListener(type, check, true);
    }
    unwatch = () => {
      for (const type of ABANDON_EVENTS) {
        const target = type === "visibilitychange" ? document : window;
        target.removeEventListener(type, check, true);
      }
      unwatch = () => {};
    };
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
      if (event.buttons === 0) {
        if (held) abandon();
        else end(event.currentTarget, event.pointerId);
        return;
      }
      if (!held) {
        const far =
          Math.hypot(event.clientX - start.x, event.clientY - start.y) >=
          DRAG_START_PX;
        if (!far || !events) return;
        held = { row: event.currentTarget, events };
        watch();
      }
      events?.move(event.clientX, event.clientY);
    },
    onPointerUp: (event: Press, events: TabDragEvents | undefined) => {
      if (event.pointerId !== start?.pointer) return;
      const dragged = held !== undefined;
      end(event.currentTarget, event.pointerId);
      if (dragged) {
        swallowClick = true;
        events?.drop(event.clientX, event.clientY);
      }
    },
    onPointerCancel: (event: Press, events: TabDragEvents | undefined) => {
      if (event.pointerId !== start?.pointer) return;
      const dragged = held !== undefined;
      end(event.currentTarget, event.pointerId);
      if (dragged) events?.cancel();
    },
    /** True once after a drop, for the click that follows it. */
    swallowClick: () => {
      const swallow = swallowClick;
      swallowClick = false;
      return swallow;
    },
  };
}
