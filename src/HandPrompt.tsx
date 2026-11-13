import { createSignal, onCleanup, onMount, Show } from "solid-js";

import { keepInside } from "./ShortcutsSheet";

/** Asks once, over the tab, to reopen a chat started by hand as a Claude tab; nothing restarts without the click. */
export function HandPrompt(props: {
  working: boolean;
  onReopen: () => void;
  onNotNow: () => void;
}) {
  const [asking, setAsking] = createSignal(false);
  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onNotNow();
      else if (event.key === "Tab" && sheet && keepInside(sheet, event)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
    });
    sheet?.querySelector<HTMLButtonElement>("button.go")?.focus();
  });
  return (
    <div class="sheet-backdrop">
      <div
        class="sheet hand-prompt"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="hand-prompt-title"
        ref={(el) => {
          sheet = el;
        }}
      >
        <h2 id="hand-prompt-title">Started by hand</h2>
        <Show
          when={asking()}
          fallback={
            <p>
              This chat was started by typing <code>claude</code> in the shell,
              so it can't send or receive messages between accounts. Reopen it
              as a Claude tab to turn messages on; it resumes where it is.
            </p>
          }
        >
          <p>Claude is working. Stop it and reopen the chat as a Claude tab?</p>
        </Show>
        <div class="env-actions">
          <button
            type="button"
            class="secondary"
            onClick={() => {
              if (asking()) setAsking(false);
              else props.onNotNow();
            }}
          >
            {asking() ? "Cancel" : "Not now"}
          </button>
          <button
            type="button"
            class="go"
            onClick={() => {
              if (props.working && !asking()) setAsking(true);
              else props.onReopen();
            }}
          >
            {asking() ? "Stop and reopen" : "Reopen as Claude tab"}
          </button>
        </div>
      </div>
    </div>
  );
}
