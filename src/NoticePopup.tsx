import { onCleanup, onMount } from "solid-js";

import { backdropDismiss } from "./backdrop";

/** A message that must be read, such as why a chat could not move; OK, Enter or Escape closes it. */
export function NoticePopup(props: {
  title: string;
  message: string;
  onClose: () => void;
}) {
  let ok: HTMLButtonElement | undefined;
  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
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
