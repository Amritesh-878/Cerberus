import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import { addDirCommand, planFolders } from "./environment";
import {
  button,
  cleanup,
  click,
  emit,
  field,
  open,
  press,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const ALPHA = String.raw`C:\code\alpha`;
const PLANS = String.raw`C:\code\plans`;
const BETA = String.raw`C:\code\beta`;

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: ALPHA }],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: ALPHA,
      account: 1,
      dirs: [],
    },
    {
      id: 4,
      projectId: 1,
      parentId: null,
      title: "shell",
      program: "powershell",
      cwd: ALPHA,
      account: 1,
      dirs: [],
    },
  ],
  nextId: 5,
};

const PROFILES: Profiles = {
  ...SAVED,
  panel: { ...SAVED.panel, open: true },
};

/** A core whose pickers answer `picks` in turn and whose environment calls are recorded. */
function envCore(
  picks: string[],
  options: {
    refuseEnv?: string;
    holdMove?: (release: () => void) => void;
  } = {},
) {
  const pty = ptyCore();
  const writes: string[] = [];
  const saves: SavedTree[] = [];
  const calls: { cmd: string; args: unknown }[] = [];
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  return {
    pty,
    writes,
    saves,
    calls,
    core: {
      ...pty.core,
      "plugin:dialog|open": () => picks.shift() ?? null,
      tree_load: () => ({ tree: TREE, notice: null, ended: [] }),
      tree_save: (args: unknown) => {
        saves.push(field(args, "tree") as SavedTree);
      },
      pty_write: (args: unknown) => {
        writes.push(String(field(args, "data")));
      },
      panel_changes: () => ({ kind: "notRepo" }),
      panel_connectors: () => ({ servers: [], unreadable: [] }),
      panel_skills: () => [],
      ...record("tab_env", () => ["API_URL"]),
      ...record("tab_set_env", (args) => {
        if (options.refuseEnv) throw new Error(options.refuseEnv);
        return field(args, "edits");
      }),
      ...record("tab_move_folder", () =>
        options.holdMove
          ? new Promise<null>((resolve) => {
              options.holdMove?.(() => {
                resolve(null);
              });
            })
          : null,
      ),
    },
  };
}

function dialog(): HTMLElement {
  const found = document.querySelector<HTMLElement>(".env-dialog");
  if (!found) throw new Error("no dialog");
  return found;
}

function input(label: string): HTMLInputElement {
  const found =
    dialog().querySelector<HTMLInputElement>(`input[aria-label="${label}"]`) ??
    dialog().querySelector<HTMLInputElement>(".env-field input");
  if (!found) throw new Error(`no ${label} input`);
  return found;
}

function type(el: HTMLInputElement, text: string): void {
  el.value = text;
  el.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

async function editEnvironment(root: HTMLElement, tab: string): Promise<void> {
  await open(root, tab);
  click(root, "Edit environment");
  await settle();
}

const lastSave = (saves: SavedTree[], id: number) =>
  saves.at(-1)?.tabs.find((t) => t.id === id);

test("adding a folder saves it and types /add-dir into the running chat, never Enter", async () => {
  const env = envCore([PLANS]);
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "planner");

  type(input("Name"), "planner with plans");
  click(dialog(), "Add folder…");
  await settle();
  expect(dialog().textContent).toContain("plans");
  expect(dialog().textContent).toContain("/add-dir is typed into the chat");
  click(dialog(), "Save");
  await settle();

  expect(document.querySelector(".env-dialog")).toBeNull();
  expect(lastSave(env.saves, 3)).toMatchObject({
    title: "planner with plans",
    cwd: ALPHA,
    dirs: [PLANS],
  });
  expect(env.writes).toEqual([`/add-dir ${PLANS}`]);
  expect(env.calls.map((c) => c.cmd)).not.toContain("tab_move_folder");
  // No restart: the chat keeps running in the one pane it had.
  expect(env.pty.spawned).toHaveLength(1);
});

test("a later start passes the saved folders with the spawn request", async () => {
  const env = envCore([PLANS, BETA]);
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "planner");
  click(dialog(), "Add folder…");
  await settle();
  click(dialog(), "Save");
  await settle();

  click(root, "Edit environment");
  await settle();
  env.calls.length = 0;
  click(dialog(), "Change…");
  await settle();
  click(dialog(), "Save");
  await settle();
  await settle();

  const spawn = env.pty.spawned.at(-1);
  expect(field(field(spawn, "request"), "dirs")).toEqual([PLANS]);
});

test("a new main folder restarts the tab there after the core carries the chat", async () => {
  const env = envCore([BETA]);
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "planner");

  click(dialog(), "Change…");
  await settle();
  expect(dialog().textContent).toContain("restarts in the new folder");
  click(dialog(), "Save");
  await settle();
  await settle();

  const moved = env.calls.filter((c) => c.cmd === "tab_move_folder");
  expect(
    moved.map((c) => [field(c.args, "tab"), field(c.args, "folder")]),
  ).toEqual([[3, BETA]]);
  expect(env.pty.killed).toEqual([1]);
  expect(env.pty.spawned).toHaveLength(2);
  expect(field(field(env.pty.spawned[1], "request"), "cwd")).toBe(BETA);
  expect(field(env.pty.spawned[1], "tab")).toBe(3);
  expect(lastSave(env.saves, 3)?.cwd).toBe(BETA);
});

test("a working chat asks before it moves, and Back leaves it running", async () => {
  const env = envCore([BETA, BETA]);
  const root = await renderApp(() => PROFILES, env.core);
  await open(root, "planner");
  emit(env.pty.spawned, 0, { state: "working" });
  await settle();
  click(root, "Edit environment");
  await settle();

  click(dialog(), "Change…");
  await settle();
  click(dialog(), "Save");
  await settle();

  expect(dialog().textContent).toContain("The chat is working");
  click(dialog(), "Back");
  await settle();
  expect(env.calls.map((c) => c.cmd)).not.toContain("tab_move_folder");
  expect(env.pty.killed).toEqual([]);

  click(dialog(), "Save");
  await settle();
  click(dialog(), "Restart");
  await settle();
  await settle();
  expect(env.calls.map((c) => c.cmd)).toContain("tab_move_folder");
  expect(field(field(env.pty.spawned.at(-1), "request"), "cwd")).toBe(BETA);
});

test("a shell tab just restarts in the new folder and types nothing", async () => {
  const env = envCore([BETA, PLANS]);
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "shell");

  click(dialog(), "Change…");
  await settle();
  click(dialog(), "Add folder…");
  await settle();
  expect(dialog().textContent).not.toContain("/add-dir");
  click(dialog(), "Save");
  await settle();
  await settle();

  expect(env.writes).toEqual([]);
  expect(field(field(env.pty.spawned.at(-1), "request"), "cwd")).toBe(BETA);
});

test("variables are sent as edits, shown by name only, and a refusal changes nothing", async () => {
  const env = envCore([], {
    refuseEnv: "CLAUDE_CONFIG_DIR is set by Cerberus",
  });
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "planner");

  expect(dialog().textContent).toContain("API_URL");
  expect(dialog().textContent).toContain("value hidden");
  expect(dialog().textContent).toContain("plain text");
  type(input("Name"), "renamed");
  type(input("Variable name"), "CLAUDE_CONFIG_DIR");
  type(input("Variable value"), "secret-value");
  click(dialog(), "Add");
  click(dialog(), "Remove API_URL");
  // Typed but not added: Save takes it too.
  type(input("Variable name"), "MODE");
  type(input("Variable value"), "fast");
  await settle();
  const before = env.saves.length;
  click(dialog(), "Save");
  await settle();

  const sent = env.calls.find((c) => c.cmd === "tab_set_env");
  expect(field(sent?.args, "edits")).toEqual([
    { name: "API_URL", value: null },
    { name: "CLAUDE_CONFIG_DIR", value: "secret-value" },
    { name: "MODE", value: "fast" },
  ]);
  expect(dialog().textContent).toContain("set by Cerberus");
  expect(env.saves.length).toBe(before);
  expect(JSON.stringify(env.saves)).not.toContain("secret-value");
  expect(button(root, "Edit environment")).toBeTruthy();
});

test("the folder plan and the typed command", () => {
  expect(
    planFolders(
      { cwd: ALPHA, dirs: [PLANS, BETA] },
      { cwd: ALPHA, dirs: [BETA, "C:\\new"] },
    ),
  ).toEqual({ restart: false, added: ["C:\\new"], removed: [PLANS] });
  expect(
    planFolders({ cwd: ALPHA, dirs: [] }, { cwd: BETA, dirs: [] }).restart,
  ).toBe(true);
  expect(addDirCommand("C:\\a\r\nb\u001b")).toBe("/add-dir C:\\ab");
});

test("the dialog stays open while a folder move is pending, so no second move can start", async () => {
  let release: (() => void) | undefined;
  const env = envCore([BETA], {
    holdMove: (go) => {
      release = go;
    },
  });
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "planner");
  click(dialog(), "Change…");
  await settle();
  click(dialog(), "Save");
  await settle();

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  click(dialog(), "Close");
  await settle();
  expect(document.querySelector(".env-dialog")).not.toBeNull();
  expect(env.pty.spawned).toHaveLength(1);

  release?.();
  await settle();
  await settle();
  expect(document.querySelector(".env-dialog")).toBeNull();
  expect(env.calls.filter((c) => c.cmd === "tab_move_folder")).toHaveLength(1);
  expect(field(field(env.pty.spawned.at(-1), "request"), "cwd")).toBe(BETA);
});

test("app shortcuts do nothing while the dialog is open, so its tab cannot close behind it", async () => {
  const env = envCore([]);
  const root = await renderApp(() => PROFILES, env.core);
  await editEnvironment(root, "planner");

  press("W", { ctrlKey: true, shiftKey: true });
  press("w", { metaKey: true });
  await settle();

  expect(document.querySelector(".env-dialog")).not.toBeNull();
  expect(root.textContent).toContain("planner");
  expect(env.pty.killed).toEqual([]);
});
