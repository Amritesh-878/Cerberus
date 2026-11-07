import { createSignal } from "solid-js";

import type { Profiles } from "./accounts";

export interface SavedWidth {
  value: () => number;
  error: () => string | undefined;
  /** While dragging; `save` when the drag ends. */
  drag: (value: number) => void;
  save: () => void;
}

/** A size the user drags, shown at once and saved one write at a time, only the latest waiting. */
export function createSavedWidth(options: {
  saved: () => number | null | undefined;
  fallback: number;
  send: (value: number) => Promise<Profiles>;
  changed: (fresh: Profiles) => void;
}): SavedWidth {
  // Held while dragging, so the edge follows the pointer before the saved value comes back.
  const [dragged, setDragged] = createSignal<number>();
  const [error, setError] = createSignal<string>();
  const value = () => dragged() ?? options.saved() ?? options.fallback;
  let saving = false;
  let waiting: number | undefined;
  const send = (next: number) => {
    saving = true;
    options
      .send(next)
      .then(
        (saved) => {
          setError(undefined);
          options.changed(saved);
        },
        (failure: unknown) => {
          setError(String(failure));
          // A newer value still waiting gets its own try; with none, fall back to the saved one.
          if (waiting === undefined) setDragged(undefined);
        },
      )
      .finally(() => {
        saving = false;
        const after = waiting;
        waiting = undefined;
        if (after !== undefined) send(after);
      });
  };
  return {
    value,
    error,
    drag: (next) => {
      setDragged(next);
    },
    save: () => {
      if (saving) waiting = value();
      else send(value());
    },
  };
}
