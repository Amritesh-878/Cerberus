import { afterEach, expect, test } from "vitest";

import { resetText } from "./board";
import {
  activeTitle,
  cleanup,
  click,
  field,
  press,
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
    "Accounts",
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
  // The move ends what the chat left running, so the confirm says so before it happens.
  expect(
    root
      .querySelector(".menu-confirm-note")
      ?.textContent.replace(/\s+/g, " ")
      .trim(),
  ).toBe(
    "Anything it left running, such as a dev server, is stopped too. Use a service to keep one running.",
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

  const popup = document.querySelector<HTMLElement>(".notice-popup");
  expect(popup?.textContent).toContain("Could not continue on personal");
  expect(popup?.textContent).toContain(
    "Error: the other account already has this chat. The chat stays on its account.",
  );
  click(popup ?? root, "OK");
  await settle();
  expect(document.querySelector(".notice-popup")).toBeNull();
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

test("while a move is under way the tab's menu offers no move, so it moves once", async () => {
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
  expect(menuItems(root)).toEqual(["Continue on personal… (off)", "Accounts"]);
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();

  expect(app.continued).toHaveLength(1);
  finish();
  await settle();
  expect(app.pty.spawned.map((s) => field(s, "account"))).toEqual([0]);
});

test("a moving chat says where it is going on its row and in its pane until it restarts", async () => {
  const app = core();
  let finish: (() => void) | undefined;
  const root = await renderApp(() => SAVED, {
    ...app.core,
    continue_on_account: (args: unknown) => {
      app.continued.push(args);
      return new Promise<void>((resolve) => {
        finish = resolve;
      });
    },
  });
  await open(root, "Claude Code · work");

  await openMenu(root, "Claude Code · work");
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();

  expect(root.querySelector(".moving-line")?.textContent).toBe(
    "Moving to personal…",
  );
  expect(root.querySelector(".tab-view .pane-note")?.textContent).toBe(
    "Moving to personal…",
  );

  finish?.();
  await settle();

  expect(root.querySelector(".moving-line")).toBeNull();
  expect(root.querySelector(".pane-note")).toBeNull();
  expect(app.pty.spawned).toHaveLength(2);
});

// The Claude tab is the first row; a move renames it after its new account.
function showsShell(root: HTMLElement): boolean {
  return root.querySelector(".tab-row")?.querySelector(".icon-shell") != null;
}

test.each([
  ["a move resumes it on the new account", undefined, false],
  [
    "a refused move keeps it ended",
    "the other account already has this chat",
    true,
  ],
])("an ended chat: %s", async (_name, fails, stillEnded) => {
  const app = core({ fails });
  const root = await renderApp(() => SAVED, {
    ...app.core,
    tree_load: () => ({ tree: TREE, notice: null, ended: [2] }),
  });
  expect(showsShell(root)).toBe(true);

  await openMenu(root, "Claude Code · work");
  click(
    root.querySelector(".tab-menu") ?? root,
    "Continue the ended chat on personal…",
  );
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();

  expect(showsShell(root)).toBe(stillEnded);
});

test("an ended tab offers only its own saved chat, and asks first since its shell may run a claude started by hand", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, {
    ...app.core,
    tree_load: () => ({ tree: TREE, notice: null, ended: [2] }),
  });

  await openMenu(root, "Claude Code · work");
  expect(menuItems(root)).toEqual([
    "Resume chat",
    "Continue the ended chat on personal…",
    "Accounts",
  ]);
  click(
    root.querySelector(".tab-menu") ?? root,
    "Continue the ended chat on personal…",
  );
  await settle();
  expect(root.querySelector(".menu-confirm")?.textContent).toContain(
    "Close this tab's shell and continue the ended chat on personal?",
  );
  // The picked item is gone, so the keyboard lands on Continue.
  expect(document.activeElement?.textContent).toBe("Continue");
  click(root.querySelector(".menu-confirm") ?? root, "Cancel");
  await settle();

  expect(app.continued).toEqual([]);
});

async function openPill(root: HTMLElement): Promise<void> {
  await open(root, "Claude Code · work");
  root.querySelector<HTMLButtonElement>(".account-pill")?.click();
  await settle();
}

test("the account pill names the tab's account and offers the others, then Accounts", async () => {
  const root = await renderApp(() => SAVED, core().core);

  await openPill(root);

  expect(root.querySelector(".menu-account")?.textContent).toBe(
    "workuser@example.com · Pro",
  );
  expect(menuItems(root)).toEqual(["Continue on personal…", "Accounts"]);
});

test("with one account the account pill offers only Accounts, which opens the page", async () => {
  const root = await renderApp(
    () => ({ ...SAVED, accounts: SAVED.accounts.slice(1) }),
    core().core,
  );

  await openPill(root);
  expect(menuItems(root)).toEqual(["Accounts"]);
  click(root.querySelector(".tab-menu") ?? root, "Accounts");
  await settle();

  expect(root.querySelector(".tab-menu")).toBeNull();
  expect(root.querySelector("main h1")?.textContent).toBe("Accounts");
});

function removing(confirmedRemoves: unknown[]) {
  let profiles = SAVED;
  const app = core();
  return {
    app,
    profiles: () => profiles,
    core: {
      ...app.core,
      remove_account: (args: unknown) => {
        confirmedRemoves.push(field(args, "confirmed"));
        if (field(args, "confirmed") !== true)
          return { kind: "inUse", tabs: ["alpha / Claude Code · work"] };
        profiles = {
          ...SAVED,
          accounts: SAVED.accounts.filter((a) => a.id !== 1),
          retired: SAVED.accounts.filter((a) => a.id === 1),
          revision: 1,
        };
        return { kind: "removed", profiles };
      },
    },
  };
}

function accountRow(root: HTMLElement, name: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".accounts li")].find(
    (li) => li.querySelector("strong")?.textContent === name,
  );
  if (!found) throw new Error(`no ${name} account row`);
  return found;
}

test("removing an account with tabs asks first, then closes their programs and keeps the tabs", async () => {
  const asked: unknown[] = [];
  const fake = removing(asked);
  const root = await renderApp(fake.profiles, fake.core);
  await open(root, "Claude Code · work");
  click(root, "Accounts");
  await settle();

  click(accountRow(root, "work"), "Remove");
  await settle();
  expect(root.querySelector(".row-confirm")?.textContent).toContain(
    "1 tab uses work: alpha / Claude Code · work.",
  );
  expect(fake.app.pty.killed).toEqual([]);
  click(root.querySelector(".row-confirm") ?? root, "Remove work");
  await settle();

  expect(asked).toEqual([false, true]);
  expect(fake.app.pty.killed).toEqual([1]);
  expect(
    [...root.querySelectorAll(".tab-row.removed .tab-title")].map(
      (t) => t.textContent,
    ),
  ).toEqual(["Claude Code · work", "shell"]);
  await open(root, "Claude Code · work");
  expect(root.querySelector(".removed-note")?.textContent).toContain(
    "work was removed, so this tab's program closed. Its chat is saved.",
  );
  expect(fake.app.pty.spawned).toHaveLength(1);
});

test("Cancel keeps an account its tabs use, and their programs run on", async () => {
  const asked: unknown[] = [];
  const fake = removing(asked);
  const root = await renderApp(fake.profiles, fake.core);
  await open(root, "Claude Code · work");
  click(root, "Accounts");
  await settle();

  click(accountRow(root, "work"), "Remove");
  await settle();
  click(root.querySelector(".row-confirm") ?? root, "Cancel");
  await settle();

  expect(asked).toEqual([false]);
  expect(root.querySelector(".row-confirm")).toBeNull();
  expect(fake.app.pty.killed).toEqual([]);
});

test("the core's refusal to remove the last account shows on its row", async () => {
  const root = await renderApp(
    () => ({ ...SAVED, accounts: SAVED.accounts.slice(1) }),
    {
      ...core().core,
      remove_account: () => {
        throw new Error("work is the only account, and Cerberus needs one.");
      },
    },
  );
  click(root, "Accounts");
  await settle();

  click(accountRow(root, "work"), "Remove");
  await settle();

  expect(
    accountRow(root, "work").querySelector(".row-error")?.textContent,
  ).toBe("Error: work is the only account, and Cerberus needs one.");
});

test("a colour picked on an account row is saved and every dot of that account follows it", async () => {
  const saved: unknown[] = [];
  let profiles = SAVED;
  const root = await renderApp(() => profiles, {
    ...core().core,
    set_account_color: (args: unknown) => {
      saved.push(args);
      profiles = {
        ...SAVED,
        accounts: SAVED.accounts.map((a) =>
          a.id === field(args, "id")
            ? { ...a, color: String(field(args, "color")) }
            : a,
        ),
        revision: 1,
      };
      return profiles;
    },
  });
  click(root, "Accounts");
  await settle();

  accountRow(root, "work")
    .querySelector<HTMLButtonElement>(".dot-button")
    ?.click();
  await settle();
  click(accountRow(root, "work"), "Teal");
  await settle();

  expect(saved).toEqual([{ id: 1, color: "teal" }]);
  expect(
    row(root, "Claude Code · work").querySelector<HTMLElement>(".dot")?.style
      .background,
  ).toBe("#4fb5a8");
});

test("a confirm from the account pill moves the tab it was opened for, even after switching tabs", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "shell");
  await openPill(root);
  click(root.querySelector(".tab-menu") ?? root, "Continue on personal…");
  await settle();
  expect(root.querySelector(".menu-confirm")).not.toBeNull();

  press("Tab", { ctrlKey: true, shiftKey: true });
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Continue");
  await settle();

  expect(app.continued).toEqual([{ tab: 2, account: 0 }]);
});

test("a shell left on a removed account restarts on another, with no chat to move", async () => {
  const fake = removing([]);
  const root = await renderApp(fake.profiles, fake.core);
  click(root, "Accounts");
  await settle();
  click(accountRow(root, "work"), "Remove");
  await settle();
  click(root.querySelector(".row-confirm") ?? root, "Remove work");
  await settle();

  await open(root, "shell");
  expect(root.querySelector(".removed-note")?.textContent).toContain(
    "work was removed, so this shell closed.",
  );
  click(root.querySelector(".removed-note") ?? root, "Restart on personal");
  await settle();

  expect(fake.app.pty.spawned.map((s) => field(s, "account"))).toEqual([0]);
  expect(root.querySelector(".removed-note")).toBeNull();
});

test("an ended chat on a removed account offers no Resume chat, only a move", async () => {
  const app = core();
  const root = await renderApp(
    () => ({
      ...SAVED,
      accounts: SAVED.accounts.filter((a) => a.id !== 1),
      retired: SAVED.accounts.filter((a) => a.id === 1),
    }),
    {
      ...app.core,
      tree_load: () => ({ tree: TREE, notice: null, ended: [2] }),
    },
  );

  await openMenu(root, "Claude Code · work");

  expect(menuItems(root)).toEqual(["Continue on personal…", "Accounts"]);
});
