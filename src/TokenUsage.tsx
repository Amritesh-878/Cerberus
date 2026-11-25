import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { type Profiles, savedColor } from "./accounts";
import { Note } from "./Cards";
import { AccountDot } from "./icons";
import {
  type AccountTokens,
  costText,
  type Range,
  RANGES,
  tokensText,
  totalCost,
  usageTokens,
} from "./tokens";

/** Read again this often while shown, since chats keep writing. */
export const TOKENS_EVERY_MS = 5 * 60_000;

/** Tokens per account and model for today, 7 or 30 days, with an API price estimate. */
export function TokenUsage(props: { profiles: Profiles }) {
  const [counts, setCounts] = createSignal<AccountTokens[]>();
  const [error, setError] = createSignal<string>();
  const [range, setRange] = createSignal<Range>("today");
  const load = () => {
    usageTokens().then(
      (found) => {
        setCounts(found);
        setError(undefined);
      },
      (failed: unknown) => setError(String(failed)),
    );
  };
  onMount(() => {
    load();
    const timer = setInterval(load, TOKENS_EVERY_MS);
    onCleanup(() => {
      clearInterval(timer);
    });
  });
  const nameOf = (id: number) =>
    props.profiles.accounts.find((a) => a.id === id)?.name ?? "An account";
  const used = () =>
    (counts() ?? []).filter((account) => account[range()].length > 0);

  return (
    <section class="token-usage" aria-label="Tokens">
      <div class="token-head">
        <span class="token-title">Tokens</span>
        <span class="token-ranges" role="group" aria-label="Range">
          <For each={RANGES}>
            {(choice) => (
              <button
                type="button"
                class="row-button"
                aria-pressed={range() === choice.id}
                onClick={() => setRange(choice.id)}
              >
                {choice.label}
              </button>
            )}
          </For>
        </span>
      </div>
      <Show when={error()}>{(message) => <Note>{message()}</Note>}</Show>
      <Show
        when={counts()}
        fallback={
          <Show when={error() === undefined}>
            <Note>Counting tokens…</Note>
          </Show>
        }
      >
        <For each={used()} fallback={<Note>No replies in this range</Note>}>
          {(account) => (
            <div class="token-account">
              <div class="token-account-head">
                <AccountDot
                  id={account.account}
                  color={savedColor(props.profiles, account.account)}
                />
                <span class="row-main">{nameOf(account.account)}</span>
                <span class="token-cost">
                  {costText(totalCost(account[range()]))}
                </span>
              </div>
              <For each={account[range()]}>
                {(row) => (
                  <div class="token-row">
                    <span class="token-model">{row.model}</span>
                    <span class="token-cost">{costText(row.cost)}</span>
                    <span class="token-detail">{tokensText(row.tokens)}</span>
                  </div>
                )}
              </For>
            </div>
          )}
        </For>
        <p class="token-note">API price estimate, not your bill</p>
      </Show>
    </section>
  );
}
