import { createSignal, Show } from "solid-js";

import type { Profiles } from "./accounts";
import { setLive, setWarnAt } from "./live";
import { SettingSwitch } from "./SettingSwitch";

/** The "Live cards" switch and the low-limit threshold, both saved in the core. */
export function LiveSetting(props: {
  profiles: Profiles;
  onProfiles: (profiles: Profiles) => void;
}) {
  // The latest click, shown until every queued save is back, so an earlier reply cannot flip the box.
  const [choice, setChoice] = createSignal<boolean>();
  const [error, setError] = createSignal<string>();
  let unsettled = 0;
  const saved = (saving: Promise<Profiles>) =>
    saving.then(
      (profiles) => {
        setError(undefined);
        props.onProfiles(profiles);
      },
      (e: unknown) => {
        setError(String(e));
      },
    );
  const save = (on: boolean) => {
    setChoice(on);
    unsettled += 1;
    void saved(setLive(on)).finally(() => {
      unsettled -= 1;
      if (unsettled === 0) setChoice(undefined);
    });
  };

  return (
    <SettingSwitch
      class="live-setting"
      name="Live cards"
      description="Context and plan usage in the right panel, read from each chat's status line."
      newTabsOnly
      checked={choice() ?? props.profiles.live}
      onChange={save}
    >
      <label class="setting-number">
        <span>Warn when an account passes</span>
        <input
          type="number"
          min="50"
          max="99"
          step="1"
          value={props.profiles.warnAt}
          disabled={!(choice() ?? props.profiles.live)}
          onChange={(event) => {
            const input = event.currentTarget;
            const percent = input.valueAsNumber;
            // The core holds it to 50 to 99, so the box shows what was kept rather than what was typed.
            const kept = () => {
              input.value = String(props.profiles.warnAt);
            };
            if (Number.isFinite(percent))
              void saved(setWarnAt(Math.round(percent))).finally(kept);
            else kept();
          }}
        />
        <span>% of its 5-hour limit</span>
      </label>
      <Show when={error()}>
        <p class="error">Live cards setting not saved: {error()}</p>
      </Show>
    </SettingSwitch>
  );
}
