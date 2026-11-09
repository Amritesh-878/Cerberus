import { createSignal, For, onCleanup, Show } from "solid-js";

import {
  type Account,
  type AccountId,
  type Profiles,
  savedColor,
} from "./accounts";
import { AccountDot } from "./icons";
import { type LiveWarning, percentText, warningText } from "./live";
import type { Statuses } from "./statuses";
import type { TabId } from "./tree";

/** The low-limit warnings; Continue on only opens the usual confirm, so nothing moves without a click. */
export function LiveBanner(props: {
  warnings: LiveWarning[];
  profiles: Profiles;
  statuses: Statuses;
  /** The tab's title, or `undefined` once it is closed and has nothing to move. */
  tabTitle: (tab: TabId) => string | undefined;
  onContinue: (tab: TabId, account: Account, at: DOMRect) => void;
  onDismiss: (account: AccountId) => void;
}) {
  const [now, setNow] = createSignal(Date.now() / 1000);
  const timer = setInterval(() => setNow(Date.now() / 1000), 60_000);
  onCleanup(() => {
    clearInterval(timer);
  });
  const nameOf = (id: AccountId) =>
    props.profiles.accounts.find((a) => a.id === id)?.name ?? "An account";
  // Only accounts that are signed in now can take the chat.
  const offered = (warning: LiveWarning) =>
    warning.offers
      .map((id) => props.profiles.accounts.find((a) => a.id === id))
      .filter(
        (a): a is Account =>
          a !== undefined && props.statuses.get(a.id)?.state === "loggedIn",
      );

  return (
    <For each={props.warnings.filter((w) => w.resetsAt > now())}>
      {(warning) => (
        <div class="tab-alert live-warning" role="alert">
          <AccountDot
            id={warning.account}
            color={savedColor(props.profiles, warning.account)}
          />
          <span>{warningText(nameOf(warning.account), warning)}</span>
          <span class="spacer" />
          <Show when={props.tabTitle(warning.tab)}>
            {(title) => (
              <Show
                when={offered(warning).length > 0}
                fallback={
                  <span class="faint">
                    No other signed-in account is below{" "}
                    {percentText(props.profiles.warnAt)}
                  </span>
                }
              >
                <For each={offered(warning)}>
                  {(account) => (
                    <button
                      type="button"
                      title={`Move ${title()} to ${account.name}; you confirm first`}
                      onClick={(event) => {
                        props.onContinue(
                          warning.tab,
                          account,
                          event.currentTarget.getBoundingClientRect(),
                        );
                      }}
                    >
                      Continue on {account.name}
                    </button>
                  )}
                </For>
              </Show>
            )}
          </Show>
          <button
            type="button"
            onClick={() => {
              props.onDismiss(warning.account);
            }}
          >
            Dismiss
          </button>
        </div>
      )}
    </For>
  );
}
