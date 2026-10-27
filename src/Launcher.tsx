import { createResource, For, Show } from "solid-js";

import { listPrograms } from "./pty";
import { type Tab, tabFor } from "./tab";

export function Launcher(props: { onLaunch: (tab: Tab) => void }) {
  const [choices] = createResource(listPrograms);

  return (
    <main class="launcher">
      <h1>Cerberus</h1>
      <Show when={choices.error !== undefined}>
        <p class="error">Could not list programs: {String(choices.error)}</p>
      </Show>
      <ul>
        <For each={choices()}>
          {(choice) => {
            const tab = tabFor(choice);
            return (
              <li>
                <button
                  type="button"
                  disabled={tab === undefined}
                  onClick={() => {
                    if (tab) props.onLaunch(tab);
                  }}
                >
                  {choice.label}
                </button>
                <span class="program">{choice.program ?? "not installed"}</span>
              </li>
            );
          }}
        </For>
      </ul>
    </main>
  );
}
