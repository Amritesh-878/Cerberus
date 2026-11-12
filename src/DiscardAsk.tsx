import { onMount } from "solid-js";

/** Asks before a dialog with unsaved changes closes by Escape or its backdrop. */
export function DiscardAsk(props: {
  onDiscard: () => void;
  onKeep: () => void;
}) {
  let keep: HTMLButtonElement | undefined;
  // Keep editing is the safe answer, so Enter lands there.
  onMount(() => keep?.focus());
  return (
    <div class="discard-ask" role="alertdialog" aria-label="Unsaved changes">
      <p>Discard your changes?</p>
      <div class="confirm-actions">
        <button
          type="button"
          class="go"
          ref={(el) => {
            keep = el;
          }}
          onClick={() => {
            props.onKeep();
          }}
        >
          Keep editing
        </button>
        <button
          type="button"
          onClick={() => {
            props.onDiscard();
          }}
        >
          Discard
        </button>
      </div>
    </div>
  );
}
