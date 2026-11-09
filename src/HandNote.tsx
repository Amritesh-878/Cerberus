import { createSignal, Show } from "solid-js";

/** Says a chat started by hand can't send messages, and offers to reopen it as a Claude tab, asking first while it works. */
export function HandNote(props: { working: boolean; onReopen: () => void }) {
  const [asking, setAsking] = createSignal(false);
  return (
    <div class="hand-bar" role="status">
      <Show
        when={asking()}
        fallback={
          <>
            <span>Started by hand: messages need a Claude tab.</span>
            <button
              type="button"
              onClick={() => {
                if (props.working) setAsking(true);
                else props.onReopen();
              }}
            >
              Reopen as Claude tab
            </button>
          </>
        }
      >
        <span>
          Claude is working. Stop it and reopen the chat as a Claude tab?
        </span>
        <span class="hand-actions">
          <button
            type="button"
            onClick={() => {
              props.onReopen();
            }}
          >
            Stop and reopen
          </button>
          <button
            type="button"
            class="secondary"
            onClick={() => setAsking(false)}
          >
            Cancel
          </button>
        </span>
      </Show>
    </div>
  );
}
