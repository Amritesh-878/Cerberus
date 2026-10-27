import { Channel, invoke } from "@tauri-apps/api/core";

export interface SpawnSpec {
  program: string;
  args: string[];
  /** `null` starts in the user's home folder. */
  cwd: string | null;
}

export interface ProgramChoice {
  id: string;
  label: string;
  /** `null` when the program is not installed. */
  program: string | null;
  args: string[];
  pastesImages: boolean;
}

export interface Size {
  cols: number;
  rows: number;
}

export interface SessionHandlers {
  output: (bytes: Uint8Array) => void;
  exit: (code: number | null) => void;
}

interface Exited {
  exitCode: number | null;
}

export function listPrograms(): Promise<ProgramChoice[]> {
  return invoke<ProgramChoice[]>("list_programs");
}

const ACK_BATCH_BYTES = 64 * 1024;

/** A program in a pseudo-terminal; calls run in order, and those made before it starts wait. */
export class PtySession {
  readonly started: Promise<number>;
  #tail: Promise<unknown>;
  #ended = false;
  #drawn = 0;

  constructor(spec: SpawnSpec, size: Size, handlers: SessionHandlers) {
    const events = new Channel<ArrayBuffer | Exited>((event) => {
      if (event instanceof ArrayBuffer) {
        handlers.output(new Uint8Array(event));
        return;
      }
      this.#ended = true;
      handlers.exit(event.exitCode);
    });
    this.started = invoke<number>("pty_spawn", {
      request: { ...spec, ...size },
      events,
    });
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
    this.#then((id) => invoke("pty_kill", { id }));
    this.#ended = true;
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
