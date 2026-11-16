import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import {
  cleanup,
  click,
  emit,
  field,
  open,
  ptyCore,
  renderApp,
  row,
  SAVED,
  settle,
} from "./harness";
import type { SavedTab, SavedTree } from "./saved";

const desktop = vi.hoisted(() => ({ sent: [] as unknown[] }));
vi.mock("@tauri-apps/plugin-notification", async (original) => ({
  ...(await original<typeof import("@tauri-apps/plugin-notification")>()),
  isPermissionGranted: () => Promise.resolve(true),
  sendNotification: (options: unknown) => {
    desktop.sent.push(options);
  },
}));

beforeEach(() => {
  desktop.sent = [];
});
afterEach(cleanup);

const CODE = String.raw`C:\code\alpha`;
const HANDOFF = String.raw`C:\plans\completed\task-026.md`;
const LINE = `TASK-026 handoff is ready: ${HANDOFF}. PR: https://github.com/o/r/pull/64`;
const PASTED = `\x1b[200~${LINE}\x1b[201~`;

function tree(
  options: { plannerAccount?: number; handoff?: boolean } = {},
): SavedTree {
  const coder: SavedTab = {
    id: 4,
    projectId: 1,
    parentId: 3,
    title: "TASK-026 · Handoff back",
    program: "claude",
    cwd: CODE,
    account: 1,
    ...(options.handoff === false
      ? {}
      : {
          handoff: {
            taskId: "TASK-026",
            file: HANDOFF,
            since: 1_800_000_000_000,
            state: "working" as const,
          },
        }),
  };
  return {
    projects: [{ id: 1, name: "alpha", folder: CODE }],
    tabs: [
      {
        id: 3,
        projectId: 1,
        parentId: null,
        title: "planner",
        program: "claude",
        cwd: CODE,
        account: options.plannerAccount ?? 1,
      },
      coder,
    ],
    nextId: 5,
  };
}

function handoffCore(
  options: {
    tree?: SavedTree;
    /** What the core says about the handoff file, in order; the last answer repeats. */
    written?: boolean[];
    offerFails?: string;
    pick?: string | null;
  } = {},
) {
  const pty = ptyCore();
  const calls: { cmd: string; args: unknown }[] = [];
  const writes: { id: unknown; data: string }[] = [];
  const saves: SavedTree[] = [];
  const written = [...(options.written ?? [false])];
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  return {
    pty,
    calls,
    writes,
    saves,
    called: (cmd: string) => calls.filter((c) => c.cmd === cmd),
    core: {
      ...pty.core,
      tree_load: () => ({ tree: options.tree ?? tree(), notice: null }),
      tree_save: (args: unknown) => {
        saves.push(field(args, "tree") as SavedTree);
      },
      pty_write: (args: unknown) => {
        writes.push({
          id: field(args, "id"),
          data: String(field(args, "data")),
        });
      },
      ...record("handoff_written", () =>
        written.length > 1 ? written.shift() : written[0],
      ),
      ...record("handoff_message", (args) => {
        const taskId = field(args, "taskId") as string | null;
        const id = taskId ?? "TASK-027";
        const file = String(field(args, "file"));
        return {
          taskId: id,
          text: `${id} handoff is ready: ${file}. PR: https://github.com/o/r/pull/64`,
        };
      }),
      ...record("handoff_offer", () => {
        if (options.offerFails) throw new Error(options.offerFails);
        return 1;
      }),
      ...record("plugin:dialog|open", () => options.pick ?? null),
    },
  };
}

function line(root: HTMLElement): string | undefined {
  return root.querySelector(".handoff-line")?.textContent ?? undefined;
}

function savedHandoff(env: ReturnType<typeof handoffCore>) {
  return env.saves.at(-1)?.tabs.find((t) => t.id === 4)?.handoff;
}

const views = (root: HTMLElement) =>
  [...root.querySelectorAll<HTMLElement>(".tab-view")].map((v) =>
    [...v.classList].filter((c) => c !== "tab-view").join(" "),
  );

test("writing the handoff marks the coder tab ready, with a notification while it is hidden", async () => {
  const env = handoffCore({ written: [false, false, true] });
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "TASK-026 · Handoff back");
  await open(root, "planner");
  expect(line(root)).toBe("TASK-026 · working");

  emit(env.pty.spawned, 0, { state: "working" }, { state: "idle" });
  await settle();
  expect(line(root)).toBe("TASK-026 · working");
  emit(env.pty.spawned, 0, { state: "working" }, { state: "idle" });
  await settle();
  await settle();

  expect(field(env.called("handoff_written").at(-1)?.args, "file")).toBe(
    HANDOFF,
  );
  expect(line(root)).toBe("TASK-026 · handoff ready · Send");
  expect(savedHandoff(env)?.state).toBe("ready");
  expect(desktop.sent).toContainEqual({
    title: "TASK-026 · Handoff back · alpha",
    body: "Handoff ready",
    extra: { tab: 4 },
  });
  expect(env.called("handoff_message")).toEqual([]);
});

test("a handoff written while Cerberus was closed is found when the tabs load", async () => {
  const env = handoffCore({ written: [true] });
  const root = await renderApp(() => SAVED, env.core);
  await settle();

  expect(env.pty.spawned).toEqual([]);
  expect(line(root)).toBe("TASK-026 · handoff ready · Send");
});

test("on the same account the line is typed into the planner, never sent, and the planner shows beside it", async () => {
  const env = handoffCore({ written: [true] });
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");
  emit(env.pty.spawned, 0, { state: "idle" });
  await open(root, "TASK-026 · Handoff back");
  await settle();

  click(root, "Handoff ready · Send to planner");
  await settle();
  await settle();

  expect(field(env.called("handoff_message")[0]?.args, "taskId")).toBe(
    "TASK-026",
  );
  expect(env.writes).toEqual([{ id: 1, data: PASTED }]);
  expect(env.writes[0]?.data).not.toContain("\r");
  expect(env.called("handoff_offer")).toEqual([]);
  expect(views(root).sort()).toEqual(["shown split-left", "shown split-right"]);
  expect(line(root)).toBe("TASK-026 · handoff sent");
  expect(savedHandoff(env)?.state).toBe("sent");
  expect(root.querySelector(".handoff-ready")).toBeNull();
});

test("a planner that has not started yet gets the line once its chat is idle", async () => {
  const env = handoffCore({ written: [true] });
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "TASK-026 · Handoff back");

  click(root, "Handoff ready · Send to planner");
  await settle();
  await settle();

  const planner = env.pty.spawned.length - 1;
  expect(field(env.pty.spawned[planner], "account")).toBe(1);
  expect(env.writes).toEqual([]);
  emit(env.pty.spawned, planner, { state: "idle" });
  await settle();

  expect(env.writes).toEqual([{ id: planner + 1, data: PASTED }]);
});

test("on another account the line goes to the approval popup, and nothing is typed", async () => {
  const env = handoffCore({
    tree: tree({ plannerAccount: 0 }),
    written: [true],
  });
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "TASK-026 · Handoff back");

  click(root, "Handoff ready · Send to planner");
  await settle();
  await settle();

  expect(env.called("handoff_offer").map((c) => c.args)).toEqual([
    { from: 4, to: 3, text: LINE },
  ]);
  expect(env.writes).toEqual([]);
  expect(line(root)).toBe("TASK-026 · handoff sent");
});

test("a refused offer says why and leaves the handoff ready", async () => {
  const env = handoffCore({
    tree: tree({ plannerAccount: 0 }),
    written: [true],
    offerFails: "the planner tab's chat isn't linked",
  });
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "TASK-026 · Handoff back");

  click(root, "Handoff ready · Send to planner");
  await settle();
  await settle();

  expect(
    document.querySelector(".notice-popup, [role='alertdialog']")?.textContent,
  ).toContain("the planner tab's chat isn't linked");
  expect(line(root)).toBe("TASK-026 · handoff ready · Send");
});

test("a coder tab started by hand sends a handoff it chooses, starting at the newest Markdown it wrote", async () => {
  const picked = String.raw`C:\plans\completed\task-027.md`;
  const env = handoffCore({ tree: tree({ handoff: false }), pick: picked });
  const root = await renderApp(() => SAVED, env.core);
  await open(root, "planner");
  emit(env.pty.spawned, 0, { state: "idle" });
  await open(root, "TASK-026 · Handoff back");
  expect(line(root)).toBeUndefined();
  await emitEvent("live", {
    tabs: [
      {
        tab: 4,
        account: 1,
        context: null,
        model: null,
        keptTheirs: false,
        outputs: [
          {
            name: "a.ts",
            folder: "",
            path: String.raw`C:\code\alpha\a.ts`,
            at: 3,
          },
          { name: "task-027.md", folder: "", path: picked, at: 2 },
        ],
        tasks: [],
        done: [],
        finished: 0,
      },
    ],
    meters: [],
    warnings: [],
  });
  await settle();

  row(root, "TASK-026 · Handoff back").dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, clientX: 20, clientY: 40 }),
  );
  await settle();
  click(root.querySelector(".tab-menu") ?? root, "Send handoff…");
  await settle();
  await settle();
  await settle();

  const dialog = env.called("plugin:dialog|open")[0]?.args;
  expect(field(field(dialog, "options"), "defaultPath")).toBe(picked);
  expect(field(env.called("handoff_message")[0]?.args, "taskId")).toBeNull();
  expect(env.writes.map((w) => w.data)).toEqual([
    `\x1b[200~TASK-027 handoff is ready: ${picked}. PR: https://github.com/o/r/pull/64\x1b[201~`,
  ]);
  expect(line(root)).toBe("TASK-027 · handoff sent");
  expect(savedHandoff(env)).toMatchObject({
    taskId: "TASK-027",
    file: picked,
    state: "sent",
  });
});
