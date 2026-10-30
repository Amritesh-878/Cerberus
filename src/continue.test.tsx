import { afterEach, expect, test } from "vitest";

import { resetText } from "./board";
import {
  activeTitle,
  cleanup,
  click,
  field,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const FOLDER = String.raw`C:\code\alpha`;

/** One project with a Claude tab and a shell, both on "work" (account 1). */
const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: FOLDER }],
  tabs: [
    {
      id: 2,
      projectId: 1,
      parentId: null,
      title: "Claude Code · work",
      program: "claude",
      cwd: FOLDER,
      account: 1,
    },
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "shell",
      program: "powershell",
      cwd: FOLDER,
      account: 1,
    },
  ],
  nextId: 4,
};

interface Options {
  fails?: string;
  signedOut?: number[];
}

function core(options: Options = {}) {
  const pty = ptyCore();
  const continued: unknown[] = [];
  return {
    pty,
    continued,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      account_status: (args: unknown) =>
        options.signedOut?.includes(field(args, "id") as number)
          ? { state: "loggedOut", summary: "signed out" }
          : { state: "loggedIn", summary: "user@example.com · Pro" },
      continue_on_account: (args: unknown) => {
        continued.push(args);
        if (options.fails) throw new Error(options.fails);
      },
    },
  };
}

function row(root: HTMLElement, title: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".tab-row")].find(
    (r) => r.querySelector(".tab-title")?.textContent === title,
  );
  if (!found) throw new Error(`no ${title} row`);
  return found;
}

async function openMenu(root: HTMLElement, title: string): Promise<void> {
  row(root, title).dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, clientX: 20, clientY: 40 }),
  );
  await settle();
}

function menuItems(root: HTMLElement): string[] {
  return [
    ...root.querySelectorAll<HTMLButtonElement>(".tab-menu [role=menuitem]"),
  ].map((b) => `${b.textContent.trim()}${b.disabled ? " (off)" : ""}`);
}

async function open(root: HTMLElement, title: string): Promise<void> {
  row(root, title).querySelector<HTMLButtonElement>(".tab-select")?.click();
  await settle();
}

test("a Claude tab's menu offers the other accounts, and a signed-out one cannot be picked", async () => {
  const app = core({ signedOut: [2] });
  const root = await renderApp(
    () => ({
      ...SAVED,
      accounts: [
        ...SAVED.accounts,
        { id: 2, name: "spare", configDir: String.raw`C:\h\.claude-spare` },
      ],
    }),
    app.core,
  );

  await openMenu(root, "Claude Code · work");

  expect(menuItems(root)).toEqual([
    "Continue on personal…",
    "Continue on spare…signed out (off)",
  ]);
});

test("a shell row opens no menu", async () => {
  const root = await renderApp(() => SAVED, core().core);

  await openMenu(root, "shell");

  expect(root.querySelector(".tab-menu")).toBeNull();
});

test("a tab that is not running moves at once and resumes on the new account", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);

  await openMenu(root, "Claude Code · work");
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();

  expect(app.continued).toEqual([{ tab: 2, account: 0 }]);
  expect(activeTitle(root)).toBe("Claude Code · personal");
  expect(app.pty.spawned).toHaveLength(1);
  expect(field(app.pty.spawned[0], "account")).toBe(0);
  expect(field(app.pty.spawned[0], "tab")).toBe(2);
});

test("a running chat asks first, and Cancel leaves it alone", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "Claude Code · work");

  await openMenu(root, "Claude Code · work");
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();
  expect(root.querySelector(".menu-confirm")?.textContent).toContain(
    "Close this chat and continue it on personal?",
  );
  click(root.querySelector(".menu-confirm") ?? root, "Cancel");
  await settle();

  expect(app.continued).toEqual([]);
  expect(app.pty.killed).toEqual([]);
});

test("confirming stops the chat, moves it and starts it again on the new account", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "Claude Code · work");

  await openMenu(root, "Claude Code · work");
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();

  expect(app.pty.killed).toEqual([1]);
  expect(app.continued).toEqual([{ tab: 2, account: 0 }]);
  expect(app.pty.spawned.map((s) => field(s, "account"))).toEqual([1, 0]);
});

test("a refused move keeps the tab on its account, still resumable, and says why", async () => {
  const app = core({ fails: "the other account already has this chat" });
  const root = await renderApp(() => SAVED, app.core);

  await openMenu(root, "Claude Code · work");
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();

  expect(root.querySelector(".tab-alert")?.textContent).toContain(
    "Could not continue on personal: Error: the other account already has this chat",
  );
  expect(activeTitle(root)).toBe("Claude Code · work");
  expect(app.pty.spawned.map((s) => field(s, "account"))).toEqual([1]);
});

// The mocked core delivers channel messages through the callback the channel registered.
interface MockInternals {
  runCallback: (id: number, data: unknown) => void;
}

test("a limited tab offers the move on its row", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "Claude Code · work");
  const resetsAt = new Date(2026, 9, 30, 18, 40).getTime() / 1000;
  const channel = field(app.pty.spawned[0], "events") as { id: number };
  const internals = (
    window as unknown as { __TAURI_INTERNALS__: MockInternals }
  ).__TAURI_INTERNALS__;
  internals.runCallback(channel.id, {
    index: 0,
    message: { state: "limited", resetsAt },
  });
  await settle();

  const shortcut = root.querySelector<HTMLButtonElement>(".limit-continue");
  expect(shortcut?.textContent).toBe(
    `Limited until ${resetText(resetsAt)} · continue on personal`,
  );
  shortcut?.click();
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();

  expect(app.continued).toEqual([{ tab: 2, account: 0 }]);
});

test("while a move is under way the tab's menu offers nothing, so it moves once", async () => {
  const app = core();
  let finish: () => void = () => undefined;
  const root = await renderApp(() => SAVED, {
    ...app.core,
    continue_on_account: (args: unknown) => {
      app.continued.push(args);
      return new Promise<void>((done) => (finish = done));
    },
  });

  await openMenu(root, "Claude Code · work");
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();
  await openMenu(root, "Claude Code · work");
  expect(menuItems(root)).toEqual(["Continue on personal… (off)"]);
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();

  expect(app.continued).toHaveLength(1);
  finish();
  await settle();
  expect(app.pty.spawned.map((s) => field(s, "account"))).toEqual([0]);
});
