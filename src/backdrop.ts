/** Handlers for a modal's backdrop: it closes only when the press and the release both land on it. */
export interface Backdrop {
  onPointerDown: (event: PointerEvent) => void;
  onPointerUp: (event: PointerEvent) => void;
  onClick: (event: MouseEvent) => void;
}

/** A drag between the dialog and the backdrop, either way, clicks the backdrop as their common ancestor, and must not close it. */
export function backdropDismiss(dismiss: () => void): Backdrop {
  let pressedHere = false;
  let releasedHere = false;
  return {
    onPointerDown: (event) => {
      pressedHere = event.target === event.currentTarget;
      releasedHere = false;
    },
    onPointerUp: (event) => {
      releasedHere = event.target === event.currentTarget;
    },
    onClick: (event) => {
      const both = pressedHere && releasedHere;
      pressedHere = false;
      releasedHere = false;
      if (both && event.target === event.currentTarget) dismiss();
    },
  };
}
