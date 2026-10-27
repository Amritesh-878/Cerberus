import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, test } from "vitest";

import { PtySession } from "./pty";

const SPEC = { program: "cmd.exe", args: [], cwd: null };
const SIZE = { cols: 80, rows: 24 };
const HANDLERS = { output: () => undefined, exit: () => undefined };

interface Call {
  cmd: string;
  args: unknown;
}

/** Mocks the core so `pty_spawn` resolves only when the returned `start` is called. */
function mockCore(): {
  calls: Call[];
  start: (result: number | Error) => void;
} {
  const calls: Call[] = [];
  let settle: (result: number | Error) => void = () => undefined;
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd !== "pty_spawn") return null;
    return new Promise((resolve, reject) => {
      settle = (result) => {
        if (result instanceof Error) reject(result);
        else resolve(result);
      };
    });
  });
  return {
    calls,
    start: (result) => {
      settle(result);
    },
  };
}

async function settled(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

afterEach(() => {
  clearMocks();
});

test("input typed before the program starts reaches it afterwards, in order", async () => {
  const core = mockCore();
  const session = new PtySession(SPEC, SIZE, HANDLERS);

  session.write("a");
  session.resize({ cols: 100, rows: 30 });
  session.write("b");
  await settled();
  expect(core.calls.map((c) => c.cmd)).toEqual(["pty_spawn"]);

  core.start(7);
  await settled();

  expect(core.calls.slice(1)).toEqual([
    { cmd: "pty_write", args: { id: 7, data: "a" } },
    { cmd: "pty_resize", args: { id: 7, cols: 100, rows: 30 } },
    { cmd: "pty_write", args: { id: 7, data: "b" } },
  ]);
});

test("spawn sends the program, its size and an event channel", async () => {
  const core = mockCore();
  new PtySession(SPEC, SIZE, HANDLERS);
  await settled();

  expect(core.calls[0]?.args).toMatchObject({ request: { ...SPEC, ...SIZE } });
});

test("a tab closed while starting still kills its program", async () => {
  const core = mockCore();
  const session = new PtySession(SPEC, SIZE, HANDLERS);

  session.kill();
  session.write("late");
  core.start(3);
  await settled();

  expect(core.calls.slice(1)).toEqual([{ cmd: "pty_kill", args: { id: 3 } }]);
});

test("nothing is sent after the program failed to start", async () => {
  const core = mockCore();
  const session = new PtySession(SPEC, SIZE, HANDLERS);
  const failure = session.started.catch((error: unknown) => error);

  core.start(new Error("program not found: cmd.exe"));
  expect(await failure).toBeInstanceOf(Error);
  session.write("x");
  session.kill();
  await settled();

  expect(core.calls.map((c) => c.cmd)).toEqual(["pty_spawn"]);
});
