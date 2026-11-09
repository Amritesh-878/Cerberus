import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal, onCleanup } from "solid-js";

export type ServiceId = number;

export type Phase =
  "starting" | "running" | "restarting" | "crashed" | "stopped";

/** One row, as the core sends it; the command is never in it. */
export interface ServiceRow {
  id: ServiceId;
  name: string;
  phase: Phase;
  url: string | null;
  folder: string | null;
  /** A shell id; `null` follows the Shell setting. */
  shell: string | null;
  startWithApp: boolean;
  restart: boolean;
  /** Why the last start failed. */
  problem: string | null;
}

export interface ServicesView {
  folded: boolean;
  services: ServiceRow[];
}

export interface Draft {
  name: string;
  command: string;
  folder: string | null;
  shell: string | null;
  startWithApp: boolean;
  restart: boolean;
}

export const EMPTY: ServicesView = { folded: false, services: [] };

export const BLANK: Draft = {
  name: "",
  command: "",
  folder: null,
  shell: null,
  startWithApp: false,
  restart: true,
};

export function phaseText(phase: Phase): string {
  switch (phase) {
    case "starting":
      return "Starting";
    case "running":
      return "Running";
    case "restarting":
      return "Restarting";
    case "crashed":
      return "Crashed";
    case "stopped":
      return "Stopped";
  }
}

/** Whether Stop applies: it is running, or about to be. */
export function isOn(phase: Phase): boolean {
  return phase === "starting" || phase === "running" || phase === "restarting";
}

// The state that most needs a look comes first.
const ATTENTION: readonly Phase[] = [
  "crashed",
  "restarting",
  "starting",
  "running",
  "stopped",
];

/** The rail's one dot for every service; `undefined` when there are none. */
export function overallPhase(
  services: readonly ServiceRow[],
): Phase | undefined {
  return ATTENTION.find((phase) => services.some((s) => s.phase === phase));
}

/** "2 of 3 running, 1 restarting", for the rail icon's label; every state that needs a look is named. */
export function servicesSummary(services: readonly ServiceRow[]): string {
  const count = (phase: Phase) =>
    services.filter((s) => s.phase === phase).length;
  const named = (["crashed", "restarting", "starting"] as const)
    .filter((phase) => count(phase) > 0)
    .map((phase) => `${String(count(phase))} ${phase}`);
  return [
    `${String(count("running"))} of ${String(services.length)} running`,
    ...named,
  ].join(", ");
}

/** A valid TCP port as typed, or `undefined`. */
export function portOf(text: string): number | undefined {
  const port = Number(text.trim());
  return /^\d{1,5}$/.test(text.trim()) && port >= 1 && port <= 65_535
    ? port
    : undefined;
}

/** The cloudflared quick tunnel preset, to a local port. */
export function tunnelDraft(port: number, draft: Draft): Draft {
  return {
    ...draft,
    name: draft.name.trim() === "" ? `tunnel ${String(port)}` : draft.name,
    command: `cloudflared tunnel --url http://localhost:${String(port)}`,
  };
}

export function devServerDraft(draft: Draft): Draft {
  return {
    ...draft,
    name: draft.name.trim() === "" ? "dev server" : draft.name,
    command: "npm run dev",
  };
}

/** The core's latest list, kept current from its `services` events. */
export function createServicesFeed() {
  const [view, setView] = createSignal<ServicesView>(EMPTY);
  let heard = false;
  const changes = listen<ServicesView>("services", (event) => {
    heard = true;
    setView(event.payload);
  });
  invoke<ServicesView>("services_view").then(
    (first) => {
      // An event that came first is newer than this answer.
      if (!heard) setView(first);
    },
    (error: unknown) => {
      console.error("could not read the services:", error);
    },
  );
  onCleanup(() => {
    void changes.then((stop) => {
      stop();
    });
  });
  return { view, set: setView };
}

export function addService(draft: Draft): Promise<ServicesView> {
  return invoke("service_add", { draft });
}

export function updateService(
  id: ServiceId,
  draft: Draft,
): Promise<ServicesView> {
  return invoke("service_update", { id, draft });
}

export type Action = "start" | "stop" | "restart" | "remove";

export function act(action: Action, id: ServiceId): Promise<ServicesView> {
  return invoke(`service_${action}`, { id });
}

/** The command, for the edit form only. */
export function serviceCommand(id: ServiceId): Promise<string> {
  return invoke("service_command", { id });
}

export function openServiceUrl(id: ServiceId): Promise<void> {
  return invoke("service_open_url", { id });
}

export function foldServices(folded: boolean): Promise<ServicesView> {
  return invoke("services_fold", { folded });
}

/** Sends the output so far, then each new chunk, to `onBytes`; the returned function stops it. */
export function watchOutput(
  id: ServiceId,
  onBytes: (bytes: Uint8Array) => void,
): () => void {
  const output = new Channel<ArrayBuffer>();
  output.onmessage = (bytes) => {
    onBytes(new Uint8Array(bytes));
  };
  let stopped = false;
  const watching = invoke<number | null>("service_watch", { id, output });
  watching.catch((error: unknown) => {
    console.error("could not show the output:", error);
  });
  return () => {
    if (stopped) return;
    stopped = true;
    void watching.then((watch) => {
      if (watch !== null) void invoke("service_unwatch", { watch });
    });
  };
}
