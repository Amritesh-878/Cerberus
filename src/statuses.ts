import { createStore } from "solid-js/store";

import { type AccountId, accountStatus, type AuthStatus } from "./accounts";

export interface Statuses {
  /** `undefined` while the check is running. */
  get: (id: AccountId) => AuthStatus | undefined;
  refresh: (ids: AccountId[]) => void;
}

export function createStatuses(): Statuses {
  const [statuses, setStatuses] = createStore<
    Record<AccountId, AuthStatus | undefined>
  >({});
  const latest = new Map<AccountId, number>();

  return {
    get: (id) => statuses[id],
    refresh: (ids) => {
      for (const id of ids) {
        const check = (latest.get(id) ?? 0) + 1;
        latest.set(id, check);
        // A slower, older check must not overwrite the answer of a newer one.
        const settle = (status: AuthStatus) => {
          if (latest.get(id) === check) setStatuses(id, status);
        };
        setStatuses(id, undefined);
        accountStatus(id)
          .then(settle)
          .catch((error: unknown) => {
            settle({
              state: "unknown",
              summary: `status unknown: ${String(error)}`,
            });
          });
      }
    },
  };
}
