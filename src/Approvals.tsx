import { createEffect, createSignal, on, Show } from "solid-js";

import type { AccountId } from "./accounts";
import { AccountDot } from "./icons";
import { type Decided, type Decision, type PendingMessage } from "./messaging";

/** How the popup names a tab: its account, with colour, and "project / tab". */
export interface TabName {
  accountId: AccountId | undefined;
  account: string;
  color: string | undefined;
  place: string;
}

function TabLine(props: { label: string; name: TabName }) {
  return (
    <div class="approval-tab">
      <span class="approval-role">{props.label}</span>
      <Show when={props.name.accountId} fallback={<span class="dot" />}>
        {(id) => <AccountDot id={id()} color={props.name.color} />}
      </Show>
      <span class="approval-account">{props.name.account}</span>
      <span class="approval-place">{props.name.place}</span>
    </div>
  );
}

/** The first waiting message, to approve, edit or reject; nothing approves by itself, and Escape is the terminal's. */
export function Approvals(props: {
  pending: PendingMessage[];
  name: (tab: number) => TabName;
  decide: (id: number, decision: Decision) => Promise<Decided>;
  /** An approved message reached its tab or its input box, so both chats can be shown. */
  onApproved: (from: number, to: number) => void;
}) {
  const first = () => props.pending.at(0);
  const [text, setText] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  // What happened to a message that needs the user to read it, held after it leaves the queue.
  const [notice, setNotice] = createSignal<string>();
  createEffect(
    on(
      () => first()?.id,
      () => {
        setText(first()?.text ?? "");
        setError(undefined);
      },
    ),
  );
  const edited = () => {
    const message = first();
    return message !== undefined && text() !== message.text;
  };
  const choose = (message: PendingMessage, decision: Decision) => {
    setBusy(true);
    setError(undefined);
    props
      .decide(message.id, decision)
      .then(
        (decided) => {
          const to = props.name(message.to);
          if (decided.how === "delivered" || decided.how === "prefilled") {
            props.onApproved(decided.from, decided.to);
          }
          if (decided.how === "prefilled") {
            setNotice(
              `It's in the input box of ${to.account} · ${to.place}. Press Enter there to send it.`,
            );
          } else if (decided.how === "failed") {
            setNotice(`Not delivered: ${decided.reason}`);
          }
        },
        (e: unknown) => {
          setError(String(e));
        },
      )
      .finally(() => {
        setBusy(false);
      });
  };

  return (
    <Show
      when={notice()}
      fallback={
        <Show when={first()}>
          {(message) => (
            <section
              class="approval"
              role="dialog"
              aria-labelledby="approval-title"
            >
              <header class="approval-head">
                <h2 id="approval-title">Message between accounts</h2>
                <Show when={props.pending.length > 1}>
                  <span class="approval-count">
                    1 of {props.pending.length}
                  </span>
                </Show>
              </header>
              <TabLine label="From" name={props.name(message().from)} />
              <TabLine label="To" name={props.name(message().to)} />
              <textarea
                class="approval-text"
                aria-label="Message"
                value={text()}
                spellcheck={false}
                onInput={(event) => setText(event.currentTarget.value)}
              />
              <Show when={edited()}>
                <p class="approval-edited">
                  Edited. The sender is told what you changed it to.
                </p>
              </Show>
              <Show when={error()}>
                <p class="error">{error()}</p>
              </Show>
              <div class="approval-actions">
                <button
                  type="button"
                  class="secondary"
                  disabled={busy()}
                  onClick={() => {
                    choose(message(), { kind: "reject" });
                  }}
                >
                  Reject
                </button>
                <button
                  type="button"
                  disabled={busy() || text().trim() === ""}
                  onClick={() => {
                    choose(message(), { kind: "approve", text: text() });
                  }}
                >
                  {edited() ? "Approve edited" : "Approve"}
                </button>
              </div>
            </section>
          )}
        </Show>
      }
    >
      {(said) => (
        <section class="approval" role="status">
          <p>{said()}</p>
          <div class="approval-actions">
            <button
              type="button"
              onClick={() => {
                setNotice(undefined);
              }}
            >
              OK
            </button>
          </div>
        </section>
      )}
    </Show>
  );
}
