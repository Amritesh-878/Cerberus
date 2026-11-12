import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import { backdropDismiss } from "./backdrop";
import {
  type Account,
  type AccountId,
  type Profiles,
  savedColor,
} from "./accounts";
import type { Target } from "./carry";
import { AccountDot } from "./icons";
import { placeBox } from "./place";

export interface MenuPlace {
  x: number;
  y: number;
}

/** The account a tab runs on, shown at the top of its menu. */
export interface MenuAccount {
  id: AccountId;
  name: string;
  status: string;
  color: string | undefined;
}

/** The header a tab's menu shows, or none for a tab with no account. */
export function menuAccount(
  account: { id: AccountId; name: string } | null | undefined,
  profiles: Profiles,
  status: (id: AccountId) => string,
): MenuAccount | undefined {
  return account
    ? {
        id: account.id,
        name: account.name,
        status: status(account.id),
        color: savedColor(profiles, account.id),
      }
    : undefined;
}

const ITEMS = '[role="menuitem"]:not(:disabled)';

/** An ended tab moves only its own saved chat, never one started by hand in its shell, and says so. */
export function continueLabel(ended: boolean, account: string): string {
  return ended
    ? `Continue the ended chat on ${account}…`
    : `Continue on ${account}…`;
}

/** Up, Down, Home and End walk the enabled items, wrapping at the ends as a native menu does. */
function moveFocus(menu: HTMLElement, key: string): boolean {
  const items = [...menu.querySelectorAll<HTMLElement>(ITEMS)];
  if (items.length === 0) return false;
  const at = items.findIndex((item) => item === document.activeElement);
  const next =
    key === "Home"
      ? 0
      : key === "End"
        ? items.length - 1
        : key === "ArrowDown"
          ? (at + 1) % items.length
          : key === "ArrowUp"
            ? (at - 1 + items.length) % items.length
            : -1;
  if (next < 0) return false;
  items[next]?.focus();
  return true;
}

export function TabMenu(props: {
  title: string;
  /** The tab's own account; the account pill's menu shows it first. */
  account?: MenuAccount;
  targets: Target[];
  running: boolean;
  /** A move of this tab is under way, so nothing more can be picked until it ends. */
  busy: boolean;
  at: MenuPlace;
  /** Set when opened from a limited row's shortcut, which goes straight to that account. */
  chosen?: Account;
  /** The tab's chat ended, so the menu offers to start it again. */
  ended: boolean;
  onContinue: (account: Account) => void;
  onResume: () => void;
  /** Last in the menu, so a signed-out account can be signed in from there. */
  onAccounts?: () => void;
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

  // An ended tab's shell may be running a claude started by hand, which the move would close.
  const pick = (account: Account) => {
    if (props.running || props.ended) setConfirming(account);
    else props.onContinue(account);
  };
  // Kept inside the window, flipping above the click when there is no room below.
  const position = () => {
    const place = placeBox(
      {
        left: props.at.x,
        right: props.at.x,
        top: props.at.y,
        bottom: props.at.y,
      },
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
    // The menu holds focus even with nothing to pick, so Escape and the arrow keys reach it.
    (
      box?.querySelector<HTMLButtonElement>("button:not(:disabled)") ?? box
    )?.focus();
  });

  return (
    <div
      class="popover-backdrop"
      {...backdropDismiss(() => {
        props.onDismiss();
      })}
    >
      <div
        class="popover menu tab-menu"
        ref={(el) => {
          box = el;
        }}
        role="menu"
        tabIndex={-1}
        aria-label={`${props.title} menu`}
        aria-busy={props.busy}
        style={position()}
        onKeyDown={(event) => {
          if (moveFocus(event.currentTarget, event.key)) event.preventDefault();
        }}
      >
        <Show
          when={confirming()}
          fallback={
            <>
              <Show when={props.account}>
                {(account) => (
                  <>
                    <div class="menu-account">
                      <AccountDot id={account().id} color={account().color} />
                      <span class="menu-text">{account().name}</span>
                      <span class="menu-key">{account().status}</span>
                    </div>
                    <div class="menu-sep" role="separator" />
                  </>
                )}
              </Show>
              <Show when={props.ended}>
                <button
                  type="button"
                  class="menu-item"
                  role="menuitem"
                  disabled={props.busy}
                  onClick={() => {
                    props.onResume();
                  }}
                >
                  <span class="menu-text">Resume chat</span>
                </button>
                <Show when={props.targets.length > 0}>
                  <div class="menu-sep" role="separator" />
                </Show>
              </Show>
              <For each={props.targets}>
                {(target) => (
                  <button
                    type="button"
                    class="menu-item"
                    role="menuitem"
                    disabled={!target.signedIn || props.busy}
                    title={target.signedIn ? undefined : "Signed out"}
                    onClick={() => {
                      pick(target.account);
                    }}
                  >
                    <AccountDot
                      id={target.account.id}
                      color={target.account.color}
                    />
                    <span class="menu-text">
                      {continueLabel(props.ended, target.account.name)}
                    </span>
                    <Show when={!target.signedIn}>
                      <span class="menu-key">signed out</span>
                    </Show>
                  </button>
                )}
              </For>
              <Show when={props.onAccounts}>
                <Show when={props.targets.length > 0 || props.ended}>
                  <div class="menu-sep" role="separator" />
                </Show>
                <button
                  type="button"
                  class="menu-item"
                  role="menuitem"
                  onClick={() => {
                    props.onAccounts?.();
                  }}
                >
                  <span class="menu-text">Accounts</span>
                </button>
              </Show>
            </>
          }
        >
          {(account) => (
            <div
              class="menu-confirm"
              role="alertdialog"
              ref={(el) => {
                // The picked item is gone, so focus moves here for Enter and Escape.
                queueMicrotask(() => {
                  el.querySelector<HTMLButtonElement>(
                    "button:not(:disabled)",
                  )?.focus();
                });
              }}
            >
              <p>
                {props.ended
                  ? `Close this tab's shell and continue the ended chat on ${account().name}?`
                  : `Close this chat and continue it on ${account().name}?`}
              </p>
              <p class="menu-confirm-note">
                Anything it left running, such as a dev server, is stopped too.
                Use a service to keep one running.
              </p>
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
