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
      <span class="approval-name">
        <Show
          when={
            // Wrapped, since account 0 is falsy and would hide its dot.
            props.name.accountId === undefined
              ? undefined
              : { id: props.name.accountId }
          }
          fallback={<span class="dot" />}
        >
          {(account) => (
            <AccountDot id={account().id} color={props.name.color} />
          )}
        </Show>
        <span class="approval-account">{props.name.account}</span>{" "}
        <span class="approval-place">{props.name.place}</span>
      </span>
    </div>
  );
}

function waitsForEnter(where: string): string {
  return `It's in the input box of ${where}. Press Enter there to send it.`;
}

/** The first waiting message, to approve, edit or reject; nothing approves by itself, and Escape is the terminal's. */
export function Approvals(props: {
  pending: PendingMessage[];
  senderGone: number[];
  name: (tab: number) => TabName;
  decide: (id: number, decision: Decision) => Promise<Decided>;
  typeIn: (id: number) => Promise<void>;
  discard: (id: number) => Promise<void>;
  /** An approved message reached its tab or its input box, so both chats show side by side. */
  onApproved: (from: number, to: number) => void;
}) {
  const first = () => props.pending.at(0);
  const [text, setText] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  // What happened to a message that needs the user to read it, held after it leaves the queue.
  const [notice, setNotice] = createSignal<{
    text: string;
    typeIn?: { id: number; where: string; from: number; to: number };
  }>();
  createEffect(
    on(
      () => first()?.id,
      (id, shown) => {
        setText(first()?.text ?? "");
        setError(undefined);
        // The message on screen left with no decision, because the chat that sent it ended.
        if (
          shown !== undefined &&
          id !== shown &&
          notice() === undefined &&
          props.senderGone.includes(shown)
        ) {
          setNotice({
            text: "The sending chat is gone, so its message was dropped.",
          });
        }
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
          const where = `${to.account} · ${to.place}`;
          if (
            decided.how === "delivered" ||
            decided.how === "prefilled" ||
            decided.how === "unconfirmed"
          ) {
            props.onApproved(decided.from, decided.to);
          }
          if (decided.how === "prefilled") {
            setNotice({ text: waitsForEnter(where) });
          } else if (decided.how === "unconfirmed") {
            setNotice({
              text: `It went to the inbox of ${where}, but that tab hasn't shown it yet. It may still arrive.`,
              typeIn: {
                id: decided.id,
                where,
                from: decided.from,
                to: decided.to,
              },
            });
          } else if (decided.how === "failed") {
            setNotice({ text: `Not delivered: ${decided.reason}` });
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

  // Only on the user's click, since the inbox copy may still show and Enter would then send it twice.
  const typeInstead = (target: {
    id: number;
    where: string;
    from: number;
    to: number;
  }) => {
    const { id, where } = target;
    setBusy(true);
    setError(undefined);
    props
      .typeIn(id)
      .then(
        () => {
          // Typed in counts as delivered, so it brings the split back if it was closed meanwhile.
          props.onApproved(target.from, target.to);
          setNotice({
            text: `${waitsForEnter(where)} If the first copy shows up there too, clear the input box instead.`,
          });
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
                  class="primary"
                  disabled={busy() || text().trim() === ""}
                  onKeyDown={(event) => {
                    // Only a click or Space approves, so an Enter meant for a chat never sends a message.
                    if (event.key === "Enter") event.preventDefault();
                  }}
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
          <p>{said().text}</p>
          <Show when={error()}>
            <p class="error">{error()}</p>
          </Show>
          <div class="approval-actions">
            <Show when={said().typeIn}>
              {(target) => (
                <button
                  type="button"
                  class="secondary"
                  disabled={busy()}
                  onClick={() => {
                    typeInstead(target());
                  }}
                >
                  Type it in instead
                </button>
              )}
            </Show>
            <button
              type="button"
              class="primary"
              onClick={() => {
                const kept = said().typeIn;
                setNotice(undefined);
                if (kept) {
                  props.discard(kept.id).catch((e: unknown) => {
                    console.error("could not drop a kept message:", e);
                  });
                }
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
