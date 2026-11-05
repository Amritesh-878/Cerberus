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

/** `prefillOnly`: approved messages are typed into the tab for the user to send; `off`: its helper never started. */
export type MessagesNote = "prefillOnly" | "off";

export interface BridgeSnapshot {
  pending: PendingMessage[];
  notes: { tab: number; note: MessagesNote }[];
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

export const NOTE_TEXT: Record<MessagesNote, { label: string; title: string }> =
  {
    prefillOnly: {
      label: "messages: prefill only",
      title:
        "Approved messages are typed into this tab's input for you to send, since its Claude Code can't take them directly.",
    },
    off: {
      label: "messages off",
      title:
        "This tab's message helper didn't start, so it can't send or receive messages between accounts.",
    },
  };

/** The core's waiting messages and tab notes, kept current from its `bridge` event. */
export function createMessaging() {
  const [snapshot, setSnapshot] = createSignal<BridgeSnapshot>({
    pending: [],
    notes: [],
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
    pending: () => snapshot().pending,
    note: (tab: number | undefined): MessagesNote | undefined =>
      tab === undefined
        ? undefined
        : snapshot().notes.find((n) => n.tab === tab)?.note,
  };
}
