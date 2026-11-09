import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal, onCleanup } from "solid-js";

import type { AccountId, Profiles } from "./accounts";
import { resetText } from "./board";
import type { TabId } from "./tree";

export interface LiveWindow {
  /** 0 to 100. */
  used: number;
  /** Unix seconds. */
  resetsAt: number;
}

export interface TabLive {
  tab: TabId;
  account: AccountId;
  /** 0 to 100; `null` until the chat's first reply. */
  context: number | null;
  model: string | null;
  /** The user's own status line runs where Cerberus can't wrap it, so this tab reports nothing. */
  keptTheirs: boolean;
}

export interface Meter {
  account: AccountId;
  fiveHour: LiveWindow | null;
  sevenDay: LiveWindow | null;
}

export interface LiveWarning {
  account: AccountId;
  tab: TabId;
  used: number;
  resetsAt: number;
  /** Other accounts not known to be past the line. */
  offers: AccountId[];
}

export interface LiveView {
  tabs: TabLive[];
  /** Only accounts that reported; any other is unknown. */
  meters: Meter[];
  warnings: LiveWarning[];
}

export const EMPTY_VIEW: LiveView = { tabs: [], meters: [], warnings: [] };

/** Whole percent, as the cards and the warning show it. */
export function percentText(used: number): string {
  return `${String(Math.round(used))}%`;
}

/** "work is at 87%, resets 6:40pm". */
export function warningText(
  account: string,
  warning: Pick<LiveWarning, "used" | "resetsAt">,
  locale?: string,
): string {
  return `${account} is at ${percentText(warning.used)}, resets ${resetText(warning.resetsAt, locale)}`;
}

/** A window past its reset is over; Claude Code starts a new one on the next reply. */
export function current(
  window: LiveWindow | null | undefined,
  nowS: number,
): LiveWindow | null {
  return window && window.resetsAt > nowS ? window : null;
}

/** The core's latest view, kept current from its `live` events. */
export function createLiveFeed(onWarning: (warning: LiveWarning) => void) {
  const [view, setView] = createSignal<LiveView>(EMPTY_VIEW);
  let heard = false;
  const changes = listen<LiveView>("live", (event) => {
    heard = true;
    setView(event.payload);
  });
  const warnings = listen<LiveWarning>("live-warning", (event) => {
    onWarning(event.payload);
  });
  invoke<LiveView>("live_view").then(
    (first) => {
      // An event that came first is newer than this answer.
      if (!heard) setView(first);
    },
    (error: unknown) => {
      console.error("could not read the live cards:", error);
    },
  );
  onCleanup(() => {
    void changes.then((stop) => {
      stop();
    });
    void warnings.then((stop) => {
      stop();
    });
  });
  return view;
}

export function dismissWarning(account: AccountId): Promise<void> {
  return invoke("live_dismiss", { account });
}

let liveSaving: Promise<unknown> = Promise.resolve();

export function setLive(on: boolean): Promise<Profiles> {
  const saved = liveSaving.then(() => invoke<Profiles>("set_live", { on }));
  liveSaving = saved.catch(() => undefined);
  return saved;
}

export function setWarnAt(percent: number): Promise<Profiles> {
  const saved = liveSaving.then(() =>
    invoke<Profiles>("set_warn_at", { percent }),
  );
  liveSaving = saved.catch(() => undefined);
  return saved;
}
