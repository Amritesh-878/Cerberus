import { createSignal, Show } from "solid-js";

import { type Profiles, setMessages } from "./accounts";
import { SettingSwitch } from "./SettingSwitch";

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
    <SettingSwitch
      class="messages-setting"
      name="Messages between accounts"
      description="A Claude tab can message one on another account, once you approve it."
      newTabsOnly
      checked={choice() ?? props.profiles.messages}
      onChange={save}
    >
      <Show when={error()}>
        <p class="error">Messages setting not saved: {error()}</p>
      </Show>
    </SettingSwitch>
  );
}
