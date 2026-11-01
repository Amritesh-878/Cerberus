import { clampSidebar, SIDEBAR_DEFAULT } from "./widths";

const STEPS: Record<string, number> = { ArrowLeft: -16, ArrowRight: 16 };

export function SidebarEdge(props: {
  width: number;
  onWidth: (width: number) => void;
  onDone: () => void;
}) {
  const set = (width: number) => {
    props.onWidth(clampSidebar(width, innerWidth));
  };

  return (
    <div
      class="sidebar-edge"
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      aria-valuenow={props.width}
      tabIndex={0}
      title="Drag to resize, double-click to reset"
      onPointerDown={(event) => {
        event.preventDefault();
        // Captured, so the drag keeps going over the terminal's canvas.
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          set(event.clientX);
        }
      }}
      onPointerUp={(event) => {
        event.currentTarget.releasePointerCapture(event.pointerId);
        props.onDone();
      }}
      onDblClick={() => {
        set(SIDEBAR_DEFAULT);
        props.onDone();
      }}
      onKeyDown={(event) => {
        const step = STEPS[event.key];
        if (step === undefined) return;
        event.preventDefault();
        set(props.width + step);
        props.onDone();
      }}
    />
  );
}
