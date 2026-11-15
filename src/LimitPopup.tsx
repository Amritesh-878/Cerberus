import { createMemo, For, onCleanup, onMount, Show } from "solid-js";

import { type Account, type Profiles, savedColor } from "./accounts";
import { backdropDismiss } from "./backdrop";
import { AccountDot } from "./icons";
import { type LimitHit, limitText, signedInOffers } from "./limits";
import { percentText } from "./live";
import { keepInside } from "./ShortcutsSheet";
import type { Statuses } from "./statuses";

/** A chat hit its limit: one click moves it to an account below the line, and nothing moves without that click. */
export function LimitPopup(props: {
  hit: LimitHit;
  /** The chat's tab title, named in the question. */
  title: string;
  profiles: Profiles;
  statuses: Statuses;
  onContinue: (account: Account) => void;
  onNotNow: () => void;
}) {
  const offered = createMemo(() =>
    signedInOffers(props.hit.offers, props.profiles, props.statuses),
  );
  const name = () =>
    props.profiles.accounts.find((a) => a.id === props.hit.account)?.name ??
    "This account";

  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    const opener = document.activeElement;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onNotNow();
      else if (event.key === "Tab" && sheet && keepInside(sheet, event)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    });
    sheet?.querySelector<HTMLButtonElement>("button")?.focus();
  });

  return (
    <div class="sheet-backdrop" {...backdropDismiss(props.onNotNow)}>
      <div
        class="sheet limit-popup"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="limit-title"
        aria-describedby="limit-text"
        tabIndex={-1}
        ref={(el) => {
          sheet = el;
        }}
      >
        <div class="sheet-head">
          <h2 id="limit-title">
            <AccountDot
              id={props.hit.account}
              color={savedColor(props.profiles, props.hit.account)}
            />
            {props.title} hit its limit
          </h2>
        </div>
        <p id="limit-text" class="limit-text">
          {limitText(name(), props.hit.resetsAt)}{" "}
          <Show
            when={offered().length > 0}
            fallback={`No other signed-in account is below ${percentText(props.profiles.warnAt)}.`}
          >
            Continue this chat on another account? Anything it left running
            stops, and <code>continue</code> is typed for you to send.
          </Show>
        </p>
        <div class="limit-actions">
          <For each={offered()}>
            {(account, index) => (
              <button
                type="button"
                classList={{ secondary: index() > 0 }}
                onClick={() => {
                  props.onContinue(account);
                }}
              >
                <AccountDot id={account.id} color={account.color} />
                Continue on {account.name}
              </button>
            )}
          </For>
          <button
            type="button"
            class="secondary"
            onClick={() => {
              props.onNotNow();
            }}
          >
            Not now
          </button>
        </div>
      </div>
    </div>
  );
}
