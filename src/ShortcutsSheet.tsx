import { For, onCleanup, onMount } from "solid-js";

import { backdropDismiss } from "./backdrop";
import { CloseIcon } from "./icons";
import { shortcutSheet } from "./keys";

/** Wraps Tab and Shift+Tab inside the sheet, so focus never reaches the page behind a modal. */
export function keepInside(sheet: HTMLElement, event: KeyboardEvent): boolean {
  const stops = [...sheet.querySelectorAll<HTMLElement>("button, input")];
  const first = stops[0];
  const last = stops.at(-1);
  const active = document.activeElement;
  if (!first || !last) return true;
  const target =
    active === null || !sheet.contains(active)
      ? first
      : event.shiftKey && (active === first || active === sheet)
        ? last
        : !event.shiftKey && active === last
          ? first
          : undefined;
  target?.focus();
  return target !== undefined;
}

/** Every app shortcut, over whatever is showing; Escape, a click outside or its own chord closes it. */
export function ShortcutsSheet(props: { onClose: () => void }) {
  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    // Usually the terminal, which should have the keyboard back once the sheet closes.
    const opener = document.activeElement;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
      else if (event.key === "Tab" && sheet && keepInside(sheet, event)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    });
    sheet?.focus();
  });

  return (
    <div
      class="sheet-backdrop"
      {...backdropDismiss(() => {
        props.onClose();
      })}
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
