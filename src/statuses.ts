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

  return {
    get: (id) => statuses[id],
    refresh: (ids) => {
      for (const id of ids) {
        setStatuses(id, undefined);
        accountStatus(id)
          .then((status) => {
            setStatuses(id, status);
          })
          .catch((error: unknown) => {
            setStatuses(id, {
              state: "unknown",
              summary: `status unknown: ${String(error)}`,
            });
          });
      }
    },
  };
}
