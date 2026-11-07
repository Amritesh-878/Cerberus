import { type JSX, Show } from "solid-js";

/** A checkbox beside a short name with a one-line description; the whole row toggles it. */
export function SettingSwitch(props: {
  class: string;
  name: string;
  description: string;
  /** Set for settings that reach only tabs started after a change. */
  newTabsOnly?: boolean;
  checked: boolean;
  onChange: (on: boolean) => void;
  /** Shown under the row, such as a save error. */
  children?: JSX.Element;
}) {
  return (
    <div class={`setting-switch ${props.class}`}>
      <label>
        <input
          type="checkbox"
          checked={props.checked}
          onChange={(event) => {
            props.onChange(event.currentTarget.checked);
          }}
        />
        <span class="setting-text">
          <span class="setting-name">{props.name}</span>
          <span class="setting-description">{props.description}</span>
          <Show when={props.newTabsOnly}>
            <span class="setting-description">
              Applies to tabs started after a change.
            </span>
          </Show>
        </span>
      </label>
      {props.children}
    </div>
  );
}
