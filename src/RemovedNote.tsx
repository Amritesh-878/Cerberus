import { createSignal, For, Show } from "solid-js";

import { type Account, OTHER_ORGANIZATION } from "./accounts";

/** A tab whose account was removed: one click moves it to the default account, or another picked below. */
export function RemovedNote(props: {
  /** The removed account's name. */
  name: string;
  /** A Claude tab keeps its chat; a shell just starts again on the account it moves to. */
  chat: boolean;
  /** The default account, or the first one when the default is the removed one. */
  to: Account | undefined;
  accounts: Account[];
  crosses: (to: Account) => boolean;
  onMove: (to: Account) => void;
}) {
  const [choosing, setChoosing] = createSignal(false);
  const others = () => props.accounts.filter((a) => a.id !== props.to?.id);
  return (
    <div class="pane-note removed-note">
      <p>
        {props.chat
          ? `${props.name} was removed. This chat is saved.`
          : `${props.name} was removed.`}
      </p>
      <div class="removed-actions">
        <Show when={props.to}>
          {(to) => (
            <button
              type="button"
              class="go"
              onClick={() => {
                props.onMove(to());
              }}
            >
              Move to {to().name}
            </button>
          )}
        </Show>
        <Show when={others().length > 0}>
          <button
            type="button"
            class="secondary"
            aria-expanded={choosing()}
            onClick={() => setChoosing((open) => !open)}
          >
            Other account…
          </button>
        </Show>
      </div>
      <Show when={choosing()}>
        <div class="removed-actions">
          <For each={others()}>
            {(account) => (
              <button
                type="button"
                class="secondary"
                onClick={() => {
                  props.onMove(account);
                }}
              >
                Move to {account.name}
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.chat && props.to && props.crosses(props.to)}>
        <p class="removed-fine">{OTHER_ORGANIZATION}</p>
      </Show>
    </div>
  );
}
