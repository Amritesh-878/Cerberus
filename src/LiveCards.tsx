import { createSignal, For, onCleanup, Show } from "solid-js";

import { type Account, type Profiles, savedColor } from "./accounts";
import { resetText } from "./board";
import { Card, Note } from "./Cards";
import { AccountDot } from "./icons";
import {
  current,
  type LiveWindow,
  type Meter,
  percentText,
  type TabLive,
} from "./live";
import type { PanelState } from "./panelPrefs";

/** Seconds since the epoch, ticking once a minute so a window that resets leaves the meter. */
function createClock() {
  const read = () => Math.floor(Date.now() / 1000);
  const [now, setNow] = createSignal(read());
  const timer = setInterval(() => setNow(read()), 60_000);
  onCleanup(() => {
    clearInterval(timer);
  });
  return now;
}

function Bar(props: { used: number; high?: boolean; label: string }) {
  return (
    <div
      class="meter"
      classList={{ high: props.high === true }}
      role="meter"
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(props.used)}
    >
      <span style={{ width: `${String(Math.min(100, props.used))}%` }} />
    </div>
  );
}

/** The shown chat's context use, and a Compact that types `/compact` for the user to send. */
export function ContextCard(props: {
  live: TabLive | undefined;
  liveOn: boolean;
  canType: boolean;
  state: PanelState;
  onCompact: () => void;
}) {
  const context = () => props.live?.context ?? null;
  return (
    <Card
      id="context"
      title="Context"
      meta={(() => {
        const used = context();
        return used === null ? undefined : percentText(used);
      })()}
      state={props.state}
      action={
        <button
          type="button"
          class="card-action text"
          disabled={!props.canType}
          title={
            props.canType
              ? "Types /compact into the chat; press Enter there to send it"
              : "Open the running chat to compact it"
          }
          onClick={() => {
            props.onCompact();
          }}
        >
          Compact
        </button>
      }
    >
      <Show
        when={props.liveOn}
        fallback={<Note>Live cards are off in Accounts</Note>}
      >
        <Show
          when={props.live}
          fallback={<Note>Restart the chat to see its context here</Note>}
        >
          {(live) => (
            <Show
              when={!live().keptTheirs}
              fallback={
                <Note>
                  Your own status line runs in PowerShell, so this chat can't
                  report here
                </Note>
              }
            >
              <Show
                // Wrapped, since a bare 0% would read as false and hide the bar.
                when={(() => {
                  const used = context();
                  return used === null ? undefined : { used };
                })()}
                fallback={<Note>Shows after the chat's first reply</Note>}
              >
                {(found) => (
                  <>
                    <Bar used={found().used} label="Context used" />
                    <Show when={live().model}>
                      {(model) => <p class="card-note faint">{model()}</p>}
                    </Show>
                  </>
                )}
              </Show>
            </Show>
          )}
        </Show>
      </Show>
    </Card>
  );
}

function WindowLine(props: {
  name: string;
  window: LiveWindow | null;
  warnAt?: number;
}) {
  return (
    <div class="usage-line">
      <span class="usage-name">{props.name}</span>
      <Show
        when={props.window}
        fallback={<span class="usage-unknown">unknown</span>}
      >
        {(w) => (
          <>
            <Bar
              used={w().used}
              high={props.warnAt !== undefined && w().used >= props.warnAt}
              label={`${props.name} used`}
            />
            <span class="usage-meta">
              {percentText(w().used)} · resets {resetText(w().resetsAt)}
            </span>
          </>
        )}
      </Show>
    </div>
  );
}

/** One row per account: its 5-hour and 7-day use from the newest report of any of its tabs. */
export function UsageCard(props: {
  profiles: Profiles;
  meters: Meter[];
  state: PanelState;
}) {
  const now = createClock();
  const meterOf = (account: Account) =>
    props.meters.find((m) => m.account === account.id);
  return (
    <Card
      id="usage"
      title="Usage"
      subtitle="Plan limits, from each account's latest reply"
      state={props.state}
    >
      <Show
        when={props.profiles.live}
        fallback={<Note>Live cards are off in Accounts</Note>}
      >
        <For each={props.profiles.accounts} fallback={<Note>No accounts</Note>}>
          {(account) => (
            <div class="usage-account">
              <div class="usage-head">
                <AccountDot
                  id={account.id}
                  color={savedColor(props.profiles, account.id)}
                />
                <span class="row-main">{account.name}</span>
              </div>
              <WindowLine
                name="5-hour"
                window={current(meterOf(account)?.fiveHour, now())}
                warnAt={props.profiles.warnAt}
              />
              <WindowLine
                name="7-day"
                window={current(meterOf(account)?.sevenDay, now())}
              />
            </div>
          )}
        </For>
      </Show>
    </Card>
  );
}
