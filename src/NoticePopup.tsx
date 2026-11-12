import { onCleanup, onMount } from "solid-js";

import { backdropDismiss } from "./backdrop";
import { keepInside } from "./ShortcutsSheet";

/** A message that must be read, such as why a chat could not move; OK, Enter or Escape closes it. */
export function NoticePopup(props: {
  title: string;
  message: string;
  onClose: () => void;
}) {
  let ok: HTMLButtonElement | undefined;
  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    const before = document.activeElement;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
      else if (event.key === "Tab" && sheet && keepInside(sheet, event)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      // The button that opened it may be gone with the move; the terminal then takes the keys.
      if (before instanceof HTMLElement && before.isConnected) before.focus();
      else
        document
          .querySelector<HTMLElement>(".tab-view.shown .xterm-helper-textarea")
          ?.focus();
    });
    ok?.focus();
  });
  return (
    <div
      class="sheet-backdrop"
      {...backdropDismiss(() => {
        props.onClose();
      })}
    >
      <div
        class="sheet notice-popup"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="notice-title"
        ref={(el) => {
          sheet = el;
        }}
      >
        <h2 id="notice-title">{props.title}</h2>
        <p>{props.message}</p>
        <div class="env-actions">
          <button
            type="button"
            ref={(el) => {
              ok = el;
            }}
            onClick={() => {
              props.onClose();
            }}
          >
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
