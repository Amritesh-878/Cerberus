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

/** A program in a pseudo-terminal; calls run in order, and those made before it starts wait. */
export class PtySession {
  readonly started: Promise<number>;
  #tail: Promise<unknown>;
  #ended = false;

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
