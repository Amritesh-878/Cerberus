import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal, onCleanup, onMount } from "solid-js";

/** A message from one tab to a tab on another account, waiting for the user. */
export interface PendingMessage {
  id: number;
  from: number;
  to: number;
  text: string;
}

/** `prefillOnly`: approved messages are typed into the tab for the user to send; `off`: its helper never started or ended. */
export type MessagesNote = "prefillOnly" | "off";

/** Why a tab's messages are prefill only, as the core found it. */
export type NoteWhy = "platform" | "noInbox" | "version";

export interface TabNote {
  tab: number;
  note: MessagesNote;
  why?: NoteWhy | null;
}

export interface BridgeSnapshot {
  pending: PendingMessage[];
  notes: TabNote[];
  /** Recent messages dropped because the chat that sent them is gone. */
  senderGone: number[];
}

export type Decision = { kind: "approve"; text: string } | { kind: "reject" };

export type Decided =
  | { how: "delivered"; from: number; to: number }
  | { how: "prefilled"; from: number; to: number }
  | { how: "unconfirmed"; id: number; from: number; to: number }
  | { how: "rejected" }
  | { how: "failed"; reason: string };

export function decide(id: number, decision: Decision): Promise<Decided> {
  return invoke<Decided>("bridge_decide", { id, decision });
}

/** Types a message that reached an inbox but never showed into the target's input box. */
export function typeIn(id: number): Promise<void> {
  return invoke("bridge_type_in", { id });
}

/** Drops the text kept for **Type it in instead** once the user closes its notice. */
export function discard(id: number): Promise<void> {
  return invoke("bridge_discard", { id });
}

const LABELS: Record<MessagesNote, string> = {
  prefillOnly: "messages: prefill only",
  off: "messages off",
};

const TYPED_IN =
  "Approved messages are typed into this tab's input for you to send";

const TITLES: Record<NoteWhy | "off" | "unknown", string> = {
  platform: `${TYPED_IN}. Delivering them straight to the chat is checked on Windows only so far.`,
  noInbox: `${TYPED_IN}, since this tab's Claude Code reported no inbox of its own.`,
  version: `${TYPED_IN}, since this tab's Claude Code is older than 2.1.317. Update Claude Code, then reopen the tab.`,
  unknown: `${TYPED_IN}, since its Claude Code can't take them directly.`,
  off: "This tab's message helper didn't start or has ended, so it can't send or receive messages between accounts.",
};

/** The note's label and the tooltip that says why, in plain words. */
export function noteText(note: TabNote): { label: string; title: string } {
  const title =
    note.note === "off" ? TITLES.off : TITLES[note.why ?? "unknown"];
  return { label: LABELS[note.note], title };
}

/** The core's waiting messages and tab notes, kept current from its `bridge` event. */
export function createMessaging() {
  const [snapshot, setSnapshot] = createSignal<BridgeSnapshot>({
    pending: [],
    notes: [],
    senderGone: [],
  });
  onMount(() => {
    // An event that lands first is newer than the snapshot asked for, so it wins.
    let heard = false;
    const listening = listen<BridgeSnapshot>("bridge", (event) => {
      heard = true;
      setSnapshot(event.payload);
    });
    void listening.then(() =>
      invoke<BridgeSnapshot | null>("bridge_snapshot").then(
        (first) => {
          if (!heard && first) setSnapshot(first);
        },
        (error: unknown) => {
          console.error("messages between accounts unavailable:", error);
        },
      ),
    );
    onCleanup(() => {
      void listening.then((stop) => {
        stop();
      });
    });
  });
  return {
    /** The latest report, a new object on every change. */
    snapshot,
    pending: () => snapshot().pending,
    senderGone: () => snapshot().senderGone,
    note: (tab: number | undefined): TabNote | undefined =>
      tab === undefined
        ? undefined
        : snapshot().notes.find((n) => n.tab === tab),
  };
}
