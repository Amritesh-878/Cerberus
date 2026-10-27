import { createSignal, Show } from "solid-js";

import { Launcher } from "./Launcher";
import type { Tab } from "./tab";
import { TerminalView } from "./Terminal";

export function App() {
  const [tab, setTab] = createSignal<Tab>();

  return (
    <Show when={tab()} fallback={<Launcher onLaunch={setTab} />}>
      {(open) => (
        <section class="tab">
          <header class="tab-bar">
            <span>{open().label}</span>
            <button
              type="button"
              aria-label="Close tab"
              onClick={() => {
                setTab(undefined);
              }}
            >
              ×
            </button>
          </header>
          <TerminalView spec={open().spec} />
        </section>
      )}
    </Show>
  );
}
