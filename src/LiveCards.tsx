import { createSignal, For, onCleanup, Show } from "solid-js";

import { type Account, type Profiles, savedColor } from "./accounts";
import { resetText } from "./board";
import { Card, Note } from "./Cards";
import { AccountDot } from "./icons";
import {
  windowState,
  type WindowState,
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

function Bar(props: {
  used: number;
  high?: boolean;
  label: string;
  title?: string;
}) {
  return (
    <div
      class="meter"
      title={props.title}
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

/** The shown chat's context use in one row, and a Compact that types `/compact` for the user to send. */
export function ContextCard(props: {
  live: TabLive | undefined;
  liveOn: boolean;
  canType: boolean;
  state: PanelState;
  onCompact: () => void;
}) {
  const context = () => props.live?.context ?? null;
  // Why there is no meter yet, or nothing when there is one.
  const why = () => {
    if (!props.liveOn) return "Live cards are off in Accounts";
    const live = props.live;
    if (!live) return "Restart the chat to see its context here";
    if (live.keptTheirs) {
      return "Your own status line runs in PowerShell, so this chat can't report here";
    }
    return context() === null
      ? "Shows after the chat's first reply"
      : undefined;
  };
  return (
    <Card
      id="context"
      title="Context"
      fixed
      meta={(() => {
        const used = context();
        return used === null || why() !== undefined
          ? undefined
          : percentText(used);
      })()}
      inline={
        <Show
          // Wrapped, since a bare 0% would read as false and hide the bar.
          when={(() => {
            const used = context();
            return used === null || why() !== undefined ? undefined : { used };
          })()}
        >
          {(found) => (
            <Bar
              used={found().used}
              label="Context used"
              title={props.live?.model ?? undefined}
            />
          )}
        </Show>
      }
      action={
        <button
          type="button"
          class="row-button card-compact"
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
      note={why()}
      state={props.state}
    />
  );
}

function WindowLine(props: {
  name: string;
  state: WindowState;
  warnAt?: number;
}) {
  const known = () => (props.state.kind === "known" ? props.state : undefined);
  return (
    <div class="usage-line" classList={{ kept: known()?.asOf != null }}>
      <span class="usage-name">{props.name}</span>
      <Show
        when={known()}
        fallback={
          <span class="usage-unknown">
            {props.state.kind === "reset" ? "reset" : "unknown"}
          </span>
        }
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
            <Show when={w().asOf}>
              {(asOf) => (
                <span class="usage-asof">
                  as of {resetText(Math.floor(asOf() / 1000))}
                </span>
              )}
            </Show>
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
      subtitle="Plan limits; an account updates after a reply in one of its tabs"
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
                state={windowState(meterOf(account)?.fiveHour, now())}
                warnAt={props.profiles.warnAt}
              />
              <WindowLine
                name="7-day"
                state={windowState(meterOf(account)?.sevenDay, now())}
              />
            </div>
          )}
        </For>
      </Show>
    </Card>
  );
}
