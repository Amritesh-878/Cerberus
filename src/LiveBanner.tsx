import { createSignal, For, onCleanup, Show } from "solid-js";

import { type AccountId, type Profiles, savedColor } from "./accounts";
import { AccountDot } from "./icons";
import { type LiveWarning, warningText } from "./live";

/** The low-limit warnings whose popup was put off: Show opens it again over its tab, Dismiss ends it for the window. */
export function LiveBanner(props: {
  warnings: LiveWarning[];
  profiles: Profiles;
  /** The warned tab's title, or `undefined` when Show could open no move options for it. */
  showable: (warning: LiveWarning) => string | undefined;
  onShow: (warning: LiveWarning) => void;
  onDismiss: (account: AccountId) => void;
}) {
  const [now, setNow] = createSignal(Date.now() / 1000);
  const timer = setInterval(() => setNow(Date.now() / 1000), 60_000);
  onCleanup(() => {
    clearInterval(timer);
  });
  const nameOf = (id: AccountId) =>
    props.profiles.accounts.find((a) => a.id === id)?.name ?? "An account";

  return (
    <For each={props.warnings.filter((w) => w.resetsAt > now())}>
      {(warning) => (
        <div class="tab-alert live-warning" role="status">
          <AccountDot
            id={warning.account}
            color={savedColor(props.profiles, warning.account)}
          />
          <span>{warningText(nameOf(warning.account), warning)}</span>
          <span class="spacer" />
          <Show when={props.showable(warning)}>
            {(title) => (
              <button
                type="button"
                title={`Show the move options for ${title()}`}
                onClick={() => {
                  props.onShow(warning);
                }}
              >
                Show
              </button>
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
