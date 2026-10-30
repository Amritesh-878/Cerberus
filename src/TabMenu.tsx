import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import type { Account } from "./accounts";
import type { Target } from "./carry";
import { placeBox } from "./place";

export interface MenuPlace {
  x: number;
  y: number;
}

export function TabMenu(props: {
  title: string;
  targets: Target[];
  running: boolean;
  /** A move of this tab is under way, so nothing more can be picked until it ends. */
  busy: boolean;
  at: MenuPlace;
  /** Set when opened from a limited row's shortcut, which goes straight to that account. */
  chosen?: Account;
  onContinue: (account: Account) => void;
  onDismiss: () => void;
}) {
  // Read once: the shortcut opens the menu already at its confirm step.
  const [confirming, setConfirming] = createSignal(untrack(() => props.chosen));
  const [size, setSize] = createSignal({ width: 0, height: 0 });
  const [viewport, setViewport] = createSignal({
    width: innerWidth,
    height: innerHeight,
  });
  let box: HTMLDivElement | undefined;

  const pick = (account: Account) => {
    if (props.running) setConfirming(account);
    else props.onContinue(account);
  };
  // Kept inside the window, flipping above the click when there is no room below.
  const position = () => {
    const place = placeBox(
      { left: props.at.x, top: props.at.y, bottom: props.at.y },
      size(),
      viewport(),
    );
    return { left: `${String(place.left)}px`, top: `${String(place.top)}px` };
  };

  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onDismiss();
    };
    const onResize = () => {
      setViewport({ width: innerWidth, height: innerHeight });
    };
    const observer = new ResizeObserver(() => {
      setSize({
        width: box?.offsetWidth ?? 0,
        height: box?.offsetHeight ?? 0,
      });
    });
    if (box) observer.observe(box);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onResize);
    onCleanup(() => {
      observer.disconnect();
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onResize);
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
        aria-busy={props.busy}
        style={position()}
      >
        <Show
          when={confirming()}
          fallback={
            <For each={props.targets}>
              {(target) => (
                <button
                  type="button"
                  role="menuitem"
                  disabled={!target.signedIn || props.busy}
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
                  class="go"
                  disabled={props.busy}
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
