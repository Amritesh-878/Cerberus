import { createSignal } from "solid-js";

import { type Profiles, setAlerts } from "./accounts";

export interface AlertsSetting {
  on: () => boolean;
  save: (on: boolean) => void;
  error: () => string | undefined;
}

/** Held by the app, not the Accounts screen, so a pending choice and a late failure outlive the screen. */
export function createAlertsSetting(
  saved: () => Profiles | undefined,
  changed: (fresh: Profiles) => void,
): AlertsSetting {
  // The latest choice, used until every queued save is back, so an earlier reply cannot undo it.
  const [choice, setChoice] = createSignal<boolean>();
  const [error, setError] = createSignal<string>();
  let unsettled = 0;
  return {
    on: () => choice() ?? saved()?.alerts ?? true,
    error,
    save: (on) => {
      setChoice(on);
      unsettled += 1;
      setAlerts(on)
        .then(
          (fresh) => {
            setError(undefined);
            changed(fresh);
          },
          (e: unknown) => {
            setError(String(e));
          },
        )
        .finally(() => {
          unsettled -= 1;
          if (unsettled === 0) setChoice(undefined);
        });
    },
  };
}
