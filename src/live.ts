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
  /** The user's own status line runs where Cerberus can't wrap it, so this tab reports no context. */
  keptTheirs: boolean;
  /** Files the chat wrote, newest first. */
  outputs: Output[];
  /** Background shells and subagents still running, oldest first. */
  tasks: RunningTask[];
  finished: number;
}

export interface Output {
  name: string;
  /** Relative to the tab's folder when inside it; empty for the folder itself. */
  folder: string;
  path: string;
  /** Milliseconds since the epoch. */
  at: number;
}

export interface RunningTask {
  id: string;
  kind: "shell" | "subagent";
  /** `shell`, or the subagent's type. */
  label: string;
  /** Milliseconds since the epoch when Cerberus first saw it. */
  since: number;
}

/** "45s", "3m 12s", "1h 04m": how long a task has run. */
export function elapsedText(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${String(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60)
    return `${String(minutes)}m ${String(seconds % 60).padStart(2, "0")}s`;
  return `${String(Math.floor(minutes / 60))}h ${String(minutes % 60).padStart(2, "0")}m`;
}

/** Opens a file the tab's chat wrote; the core refuses any other path. */
export function openOutput(tab: TabId, path: string): Promise<void> {
  return invoke("live_open_output", { tab, path });
}

/** A file the chat wrote as the core read it for the file pane. */
export type FileView =
  /** `base` is the `HEAD` version; `null` outside a repo or for an untracked file. */
  | { kind: "text"; text: string; base: string | null }
  | { kind: "tooBig"; size: number }
  | { kind: "binary" }
  | { kind: "gone" };

/** Reads a file the tab's chat wrote, with its `HEAD` version; the core refuses any other path. */
export function readOutput(tab: TabId, path: string): Promise<FileView> {
  return invoke("live_read_output", { tab, path });
}

/** Changes whenever the file does; `null` when it is gone or not the chat's. */
export function outputStamp(tab: TabId, path: string): Promise<string | null> {
  return invoke("live_output_stamp", { tab, path });
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
