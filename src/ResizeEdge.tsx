import { createSignal, onCleanup, onMount } from "solid-js";

export interface EdgeLimits {
  min: number;
  max: (windowWidth: number) => number;
  clamp: (width: number, windowWidth: number) => number;
  reset: number;
}

/** A draggable edge between two columns; `side` is where the column it sizes sits. */
export function ResizeEdge(props: {
  class: string;
  label: string;
  side: "left" | "right";
  limits: EdgeLimits;
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
  // What the window lets the column show; a width saved in a wider window can be more.
  const shown = () => props.limits.clamp(props.width, windowWidth());
  const set = (width: number) => {
    props.onWidth(props.limits.clamp(width, windowWidth()));
  };
  // A column on the right grows as the pointer and the arrow keys move left.
  const toWidth = (x: number) => (props.side === "left" ? x : innerWidth - x);
  const step = (key: string) => {
    const right = props.side === "left" ? 16 : -16;
    return key === "ArrowRight" ? right : key === "ArrowLeft" ? -right : 0;
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
      class={props.class}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      aria-valuemin={props.limits.min}
      aria-valuemax={props.limits.max(windowWidth())}
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
        if (event.pointerId === dragging) set(toWidth(event.clientX));
      }}
      onPointerUp={finish}
      onPointerCancel={finish}
      onDblClick={() => {
        set(props.limits.reset);
        props.onDone();
      }}
      onKeyDown={(event) => {
        const by = step(event.key);
        if (by === 0) return;
        event.preventDefault();
        set(shown() + by);
        props.onDone();
      }}
    />
  );
}
