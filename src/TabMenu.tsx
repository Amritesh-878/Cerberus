import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import type { Account } from "./accounts";
import type { Target } from "./carry";

export interface MenuPlace {
  x: number;
  y: number;
}

/** A tab's menu: continue its chat on another account, asking first when the chat is running. */
export function TabMenu(props: {
  title: string;
  targets: Target[];
  running: boolean;
  at: MenuPlace;
  /** Set when opened from a limited row's shortcut, which goes straight to that account. */
  chosen?: Account;
  onContinue: (account: Account) => void;
  onDismiss: () => void;
}) {
  // Read once: the shortcut opens the menu already at its confirm step.
  const [confirming, setConfirming] = createSignal(untrack(() => props.chosen));
  let box: HTMLDivElement | undefined;

  const pick = (account: Account) => {
    if (props.running) setConfirming(account);
    else props.onContinue(account);
  };

  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onDismiss();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
    });
    box?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  });

  return (
    <div
      class="popover-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onDismiss();
      }}
    >
      <div
        class="popover tab-menu"
        ref={(el) => {
          box = el;
        }}
        role="menu"
        aria-label={`${props.title} menu`}
        style={{
          left: `${String(props.at.x)}px`,
          top: `${String(props.at.y)}px`,
        }}
      >
        <Show
          when={confirming()}
          fallback={
            <For each={props.targets}>
              {(target) => (
                <button
                  type="button"
                  role="menuitem"
                  disabled={!target.signedIn}
                  title={target.signedIn ? undefined : "Signed out"}
                  onClick={() => {
                    pick(target.account);
                  }}
                >
                  Continue on {target.account.name}…
                  <Show when={!target.signedIn}>
                    <span class="menu-note">signed out</span>
                  </Show>
                </button>
              )}
            </For>
          }
        >
          {(account) => (
            <div class="menu-confirm" role="alertdialog">
              <p>Close this chat and continue it on {account().name}?</p>
              <div class="confirm-actions">
                <button
                  type="button"
                  class="primary"
                  onClick={() => {
                    props.onContinue(account());
                  }}
                >
                  Continue
                </button>
                <button
                  type="button"
                  onClick={() => {
                    props.onDismiss();
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
