import { createSignal, Index, onCleanup, Show } from "solid-js";

import { accountColor, type Profiles, savedColor } from "./accounts";
import { Card, Note } from "./Cards";
import {
  currentUse,
  meterLevel,
  percentText,
  type Pool,
  type Segment,
  segmentText,
  type TabLive,
} from "./live";
import type { PanelState } from "./panelPrefs";
import { TokenUsage } from "./TokenUsage";

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
  label: string;
  title?: string;
  /** The fill's colour; a `warn` or `full` level overrides it. */
  color?: string;
  level?: "ok" | "warn" | "full";
}) {
  return (
    <div
      class="meter"
      title={props.title}
      classList={{
        warn: props.level === "warn",
        full: props.level === "full",
      }}
      role="meter"
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(props.used)}
    >
      <span
        style={{
          width: `${String(Math.min(100, props.used))}%`,
          background: props.level === "ok" ? props.color : undefined,
        }}
      />
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
    if (!live) return "Restart the chat to show context";
    if (live.keptTheirs) {
      return "Your status line runs in PowerShell, so this chat can't report";
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
              ? "Types /compact for you to send"
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

/** One window's bar: a segment per account in its colour, the soonest reset first; a tap picks one to read. */
function PoolBar(props: {
  name: string;
  label: string;
  segments: Segment[];
  profiles: Profiles;
  warnAt: number;
  nowS: number;
  picked: string | undefined;
  onPick: (key: string | undefined) => void;
}) {
  const nameOf = (id: number) =>
    props.profiles.accounts.find((a) => a.id === id)?.name ?? "An account";
  return (
    <div class="pool-line">
      <span class="usage-name">{props.name}</span>
      <div
        class="pool-bar"
        role="group"
        aria-label={`${props.label} use by account`}
      >
        {/* By position, so a live update keeps each button, its focus and its title. */}
        <Index each={props.segments}>
          {(segment) => {
            const key = () => `${props.name}:${String(segment().account)}`;
            const used = () => currentUse(segment(), props.nowS);
            const text = () =>
              segmentText(nameOf(segment().account), segment(), props.nowS);
            const level = () => {
              const now = used();
              return now === null ? "ok" : meterLevel(now, props.warnAt);
            };
            return (
              <button
                type="button"
                class="pool-seg"
                classList={{
                  unread: used() === null,
                  kept: segment().asOf !== null,
                  picked: props.picked === key(),
                  warn: level() === "warn",
                  full: level() === "full",
                }}
                title={text()}
                aria-label={text()}
                aria-pressed={props.picked === key()}
                onClick={() => {
                  props.onPick(props.picked === key() ? undefined : key());
                }}
              >
                <span class="pool-track">
                  <span
                    class="pool-fill"
                    style={{
                      width: `${String(Math.min(100, used() ?? 0))}%`,
                      background:
                        level() === "ok"
                          ? accountColor(
                              segment().account,
                              savedColor(props.profiles, segment().account),
                            )
                          : undefined,
                    }}
                  />
                </span>
              </button>
            );
          }}
        </Index>
      </div>
    </div>
  );
}

/** Each window as one bar with a segment per account; tapping a segment says its account, use and reset. */
export function UsageCard(props: {
  profiles: Profiles;
  pool: Pool;
  state: PanelState;
}) {
  const now = createClock();
  const [picked, setPicked] = createSignal<string>();
  const pickedText = () => {
    const key = picked();
    if (key === undefined) return undefined;
    const [window, id] = key.split(":");
    const segments =
      window === "5h" ? props.pool.fiveHour : props.pool.sevenDay;
    const segment = segments.find((s) => String(s.account) === id);
    if (!segment) return undefined;
    const name =
      props.profiles.accounts.find((a) => a.id === segment.account)?.name ??
      "An account";
    return `${window === "5h" ? "5-hour" : "7-day"} · ${segmentText(name, segment, now())}`;
  };
  return (
    <Card
      id="usage"
      title="Usage"
      hint="Plan limits from each account's latest reply; an account updates after a reply in one of its tabs"
      state={props.state}
    >
      <Show
        when={props.profiles.live}
        fallback={<Note>Live cards are off in Accounts</Note>}
      >
        <Show
          when={props.profiles.accounts.length > 0}
          fallback={<Note>No accounts</Note>}
        >
          <PoolBar
            name="5h"
            label="5-hour"
            segments={props.pool.fiveHour}
            profiles={props.profiles}
            warnAt={props.profiles.warnAt}
            nowS={now()}
            picked={picked()}
            onPick={setPicked}
          />
          <PoolBar
            name="7d"
            label="7-day"
            segments={props.pool.sevenDay}
            profiles={props.profiles}
            warnAt={props.profiles.warnAt}
            nowS={now()}
            picked={picked()}
            onPick={setPicked}
          />
          <p class="pool-detail">
            {pickedText() ?? "Tap a segment for its account, use and reset."}
          </p>
        </Show>
      </Show>
      <Show when={props.profiles.accounts.length > 0}>
        <TokenUsage profiles={props.profiles} />
      </Show>
    </Card>
  );
}
