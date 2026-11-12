/** Handlers for a modal's backdrop: it closes only when the press and the release both land on it. */
export interface Backdrop {
  onPointerDown: (event: PointerEvent) => void;
  onClick: (event: MouseEvent) => void;
}

/** A text selection that starts in the dialog and ends outside it clicks the backdrop too, and must not close it. */
export function backdropDismiss(dismiss: () => void): Backdrop {
  let pressedHere = false;
  return {
    onPointerDown: (event) => {
      pressedHere = event.target === event.currentTarget;
    },
    onClick: (event) => {
      const fromHere = pressedHere;
      pressedHere = false;
      if (fromHere && event.target === event.currentTarget) dismiss();
    },
  };
}
