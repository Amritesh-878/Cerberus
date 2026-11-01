import { createSignal, onCleanup, onMount } from "solid-js";

import {
  clampSidebar,
  SIDEBAR_DEFAULT,
  SIDEBAR_MIN,
  sidebarMax,
} from "./widths";

const STEPS: Record<string, number> = { ArrowLeft: -16, ArrowRight: 16 };

export function SidebarEdge(props: {
  width: number;
  onWidth: (width: number) => void;
  onDone: () => void;
}) {
  const [windowWidth, setWindowWidth] = createSignal(innerWidth);
  onMount(() => {
    const onResize = () => {
      setWindowWidth(innerWidth);
    };
    window.addEventListener("resize", onResize);
    onCleanup(() => {
      window.removeEventListener("resize", onResize);
    });
  });
  // What the window lets the sidebar show; a width saved in a wider window can be more.
  const shown = () => clampSidebar(props.width, windowWidth());
  const set = (width: number) => {
    props.onWidth(clampSidebar(width, windowWidth()));
  };
  // The pointer that started the drag; another finger or pen neither moves nor ends it.
  let dragging: number | undefined;
  const finish = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    if (event.pointerId !== dragging) return;
    dragging = undefined;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    props.onDone();
  };

  return (
    <div
      class="sidebar-edge"
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      aria-valuemin={SIDEBAR_MIN}
      aria-valuemax={sidebarMax(windowWidth())}
      aria-valuenow={shown()}
      tabIndex={0}
      title="Drag to resize, double-click to reset"
      onPointerDown={(event) => {
        event.preventDefault();
        if (dragging !== undefined) return;
        dragging = event.pointerId;
        // Captured, so the drag keeps going over the terminal's canvas.
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (event.pointerId === dragging) set(event.clientX);
      }}
      onPointerUp={finish}
      onPointerCancel={finish}
      onDblClick={() => {
        set(SIDEBAR_DEFAULT);
        props.onDone();
      }}
      onKeyDown={(event) => {
        const step = STEPS[event.key];
        if (step === undefined) return;
        event.preventDefault();
        set(shown() + step);
        props.onDone();
      }}
    />
  );
}
