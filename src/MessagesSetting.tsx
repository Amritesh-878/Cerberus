import { createSignal, Show } from "solid-js";

import { type Profiles, setMessages } from "./accounts";

/** The "Messages between accounts" switch; it applies to Claude tabs started after a change. */
export function MessagesSetting(props: {
  profiles: Profiles;
  onProfiles: (profiles: Profiles) => void;
}) {
  // The latest click, shown until every queued save is back, so an earlier reply cannot flip the box.
  const [choice, setChoice] = createSignal<boolean>();
  const [error, setError] = createSignal<string>();
  let unsettled = 0;
  const save = (on: boolean) => {
    setChoice(on);
    unsettled += 1;
    setMessages(on)
      .then(
        (profiles) => {
          setError(undefined);
          props.onProfiles(profiles);
        },
        (e: unknown) => {
          setError(String(e));
        },
      )
      .finally(() => {
        unsettled -= 1;
        if (unsettled === 0) setChoice(undefined);
      });
  };

  return (
    <div class="alerts-setting messages-setting">
      <label>
        <input
          type="checkbox"
          checked={choice() ?? props.profiles.messages}
          onChange={(event) => {
            save(event.currentTarget.checked);
          }}
        />
        Messages between accounts: a Claude tab can send a message to a Claude
        tab on another account, and each one waits for you to approve, edit or
        reject it. It applies to Claude tabs started after a change.
      </label>
      <Show when={error()}>
        <p class="error">Messages setting not saved: {error()}</p>
      </Show>
    </div>
  );
}
