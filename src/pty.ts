import { Channel, invoke } from "@tauri-apps/api/core";

import type { LiveState } from "./board";

export interface SpawnSpec {
  program: string;
  args: string[];
  /** `null` starts in the user's home folder. */
  cwd: string | null;
  account: number | null;
}

export interface ProgramChoice {
  id: string;
  label: string;
  /** `null` when the program is not installed. */
  program: string | null;
  args: string[];
  /** Sent for a paste of only an image, so the program reads the clipboard itself. */
  imagePasteKey: string | null;
  /** Whether the Shell button can open it. */
  isShell: boolean;
  /** What a shell runs to install the program while it is missing. */
  installCommand: string | null;
  /** The shell `installCommand` is written for; `null` means the user's own. */
  installShell: string | null;
}

export interface Size {
  cols: number;
  rows: number;
}

export interface SessionHandlers {
  output: (bytes: Uint8Array) => void;
  /** `ended`: a Claude chat ended by itself, so the core runs the shell when the tab starts again. */
  exit: (code: number | null, ended: boolean) => void;
  /** A Claude tab's chat state; other programs report none. */
  state?: (state: LiveState) => void;
}

interface Exited {
  exitCode: number | null;
  ended?: boolean;
}

/** `missing`: the saved chat is gone, so a new one started; `ended`: the chat ended, so the shell runs. */
export type Resume = "new" | "resuming" | "missing" | "ended";

interface Spawned {
  id: number;
  resume: Resume;
}

export function listPrograms(): Promise<ProgramChoice[]> {
  return invoke<ProgramChoice[]>("list_programs");
}

const ACK_BATCH_BYTES = 64 * 1024;

/** A program in a pseudo-terminal; calls run in order, and those made before it starts wait. */
export class PtySession {
  readonly started: Promise<number>;
  readonly resume: Promise<Resume>;
  #tail: Promise<unknown>;
  #ended = false;
  #drawn = 0;

  /** `tab` lets the core save the tab's chat and resume it on the next start. */
  constructor(
    spec: SpawnSpec,
    size: Size,
    handlers: SessionHandlers,
    tab: number | null = null,
  ) {
    const events = new Channel<ArrayBuffer | Exited | LiveState>((event) => {
      if (event instanceof ArrayBuffer) {
        handlers.output(new Uint8Array(event));
      } else if ("state" in event) {
        // The session-file watcher can report once more after the exit; an ended tab stays done.
        if (!this.#ended) handlers.state?.(event);
      } else {
        this.#ended = true;
        handlers.exit(event.exitCode, event.ended ?? false);
      }
    });
    const { account, ...request } = spec;
    const spawned = invoke<Spawned>("pty_spawn", {
      request: { ...request, ...size },
      account,
      tab,
      events,
    });
    this.started = spawned.then((s) => s.id);
    // A failed start is reported through `started`; nothing was resumed.
    this.resume = spawned.then(
      (s) => s.resume,
      (): Resume => "new",
    );
    this.#tail = this.started.catch(() => {
      this.#ended = true;
    });
  }

  write(data: string): void {
    this.#then((id) => invoke("pty_write", { id, data }));
  }

  resize(size: Size): void {
    this.#then((id) => invoke("pty_resize", { id, ...size }));
  }

  /** Reports output the terminal has drawn, so the core keeps sending. */
  drawn(bytes: number): void {
    this.#drawn += bytes;
    if (this.#ended || this.#drawn < ACK_BATCH_BYTES) return;
    const count = this.#drawn;
    this.#drawn = 0;
    // Not queued: a write stuck on a stalled program must not hold back the credit that unstalls it.
    this.started
      .then((id) => invoke("pty_ack", { id, bytes: count }))
      .catch((error: unknown) => {
        console.error("terminal ack failed:", error);
      });
  }

  kill(): void {
    if (this.#ended) return;
    this.#ended = true;
    // Not queued: a write stuck on a program that ignores its input must not keep the tab alive.
    this.started
      .then(
        (id) => invoke("pty_kill", { id }),
        () => undefined,
      )
      .catch((error: unknown) => {
        console.error("terminal kill failed:", error);
      });
  }

  #then(call: (id: number) => Promise<unknown>): void {
    if (this.#ended) return;
    this.#tail = this.#tail
      .then(async () => call(await this.started))
      .catch((error: unknown) => {
        console.error("terminal call failed:", error);
      });
  }
}
