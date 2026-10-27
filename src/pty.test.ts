import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, test } from "vitest";

import { PtySession } from "./pty";

const SPEC = { program: "cmd.exe", args: [], cwd: null, account: 2 };
const SIZE = { cols: 80, rows: 24 };
const HANDLERS = { output: () => undefined, exit: () => undefined };

interface Call {
  cmd: string;
  args: unknown;
}

/** Mocks the core so `pty_spawn` resolves only when `start` is called; `pty_write` can hang. */
function mockCore(options: { writesHang?: boolean } = {}): {
  calls: Call[];
  start: (result: number | Error) => void;
} {
  const calls: Call[] = [];
  let settle: (result: number | Error) => void = () => undefined;
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "pty_write" && options.writesHang)
      return new Promise(() => undefined);
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

test("spawn sends the program, its size and its account apart", async () => {
  const core = mockCore();
  new PtySession(SPEC, SIZE, HANDLERS);
  await settled();

  expect(core.calls[0]?.args).toMatchObject({
    request: { program: "cmd.exe", args: [], cwd: null, ...SIZE },
    account: 2,
  });
  expect(core.calls[0]?.args).not.toHaveProperty("request.account");
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

test("drawn output is acked in batches", async () => {
  const core = mockCore();
  const session = new PtySession(SPEC, SIZE, HANDLERS);
  core.start(4);

  session.drawn(40 * 1024);
  await settled();
  expect(core.calls.filter((c) => c.cmd === "pty_ack")).toEqual([]);

  session.drawn(30 * 1024);
  await settled();
  expect(core.calls.filter((c) => c.cmd === "pty_ack")).toEqual([
    { cmd: "pty_ack", args: { id: 4, bytes: 70 * 1024 } },
  ]);
});

test("an ack is not held back by a write the program is not reading", async () => {
  const core = mockCore({ writesHang: true });
  const session = new PtySession(SPEC, SIZE, HANDLERS);
  core.start(5);

  session.write("stuck");
  await settled();
  session.drawn(64 * 1024);
  await settled();

  expect(core.calls.map((c) => c.cmd)).toEqual([
    "pty_spawn",
    "pty_write",
    "pty_ack",
  ]);
});

test("kill is not held back by a write the program is not reading", async () => {
  const core = mockCore({ writesHang: true });
  const session = new PtySession(SPEC, SIZE, HANDLERS);
  core.start(6);

  session.write("stuck");
  await settled();
  session.kill();
  await settled();

  expect(core.calls.map((c) => c.cmd)).toEqual([
    "pty_spawn",
    "pty_write",
    "pty_kill",
  ]);
});
