import { For, onCleanup, onMount } from "solid-js";

import { CloseIcon } from "./icons";
import { shortcutSheet } from "./keys";

/** Every app shortcut, over whatever is showing; Escape, a click outside or its own chord closes it. */
export function ShortcutsSheet(props: { onClose: () => void }) {
  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
    });
    sheet?.focus();
  });

  return (
    <div
      class="sheet-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        class="sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="sheet-title"
        tabIndex={-1}
        ref={(el) => {
          sheet = el;
        }}
      >
        <header class="sheet-head">
          <h2 id="sheet-title">Keyboard shortcuts</h2>
          <button
            type="button"
            class="icon-button"
            aria-label="Close"
            onClick={() => {
              props.onClose();
            }}
          >
            <CloseIcon />
          </button>
        </header>
        <For each={shortcutSheet()}>
          {(section) => (
            <section class="sheet-section">
              <h3>{section.title}</h3>
              <dl class="sheet-keys">
                <For each={section.rows}>
                  {(row) => (
                    <>
                      <dt>{row.does}</dt>
                      <dd>
                        <For each={row.keys}>{(keys) => <kbd>{keys}</kbd>}</For>
                      </dd>
                    </>
                  )}
                </For>
              </dl>
            </section>
          )}
        </For>
      </div>
    </div>
  );
}
