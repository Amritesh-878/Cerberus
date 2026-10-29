import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import type { Account, Profiles } from "./accounts";
import { App } from "./App";
import type { ProgramChoice } from "./pty";

const CHOICES: ProgramChoice[] = [
  {
    id: "claude",
    label: "Claude Code",
    program: String.raw`C:\bin\claude.exe`,
    args: [],
    pastesImages: true,
  },
  {
    id: "powershell",
    label: "PowerShell",
    program: String.raw`C:\ps\powershell.exe`,
    args: ["-NoLogo"],
    pastesImages: false,
  },
];

const EMPTY: Profiles = {
  accounts: [],
  defaultAccount: null,
  shell: null,
  startFolder: null,
  revision: 0,
};

const SAVED: Profiles = {
  accounts: [
    { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 1, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 1,
  shell: null,
  startFolder: null,
  revision: 0,
};

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  clearMocks();
});

async function settled(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

async function renderApp(
  profiles: () => Profiles,
  core: Record<string, (args: unknown) => unknown> = {},
): Promise<HTMLElement> {
  mockIPC((cmd, args) => {
    if (cmd in core) return core[cmd]?.(args);
    switch (cmd) {
      case "list_programs":
        return CHOICES;
      case "load_profiles":
        return profiles();
      case "detect_accounts":
        return [];
      case "account_status":
        return { state: "loggedIn", summary: "user@example.com · Pro" };
      default:
        return null;
    }
  });
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(() => <App />, root);
  await settled();
  await settled();
  return root;
}

test("a first run with no accounts opens the walkthrough", async () => {
  const root = await renderApp(() => EMPTY, {
    list_programs: () =>
      CHOICES.map((c) => (c.id === "claude" ? { ...c, program: null } : c)),
  });

  expect(root.querySelector("h1")?.textContent).toBe("Set up Cerberus");
  expect(root.textContent).toContain("Claude Code isn't installed yet");
});

function button(root: ParentNode, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll("button")].find(
    (b) =>
      b.textContent.trim() === text || b.getAttribute("aria-label") === text,
  );
  if (!found) throw new Error(`no ${text} button`);
  return found;
}

function click(root: ParentNode, text: string): void {
  button(root, text).click();
}

test("the walkthrough's last step asks only which account new tabs use", async () => {
  const root = await renderApp(() => EMPTY);

  click(root, "Next");
  click(root, "Next");
  await settled();

  const section = root.querySelector(".defaults");
  expect(section?.textContent).toContain("Which account should new tabs use?");
  expect(section?.querySelectorAll("select, input")).toHaveLength(1);
});

test("the accounts screen sets where the folder picker opens", async () => {
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    "plugin:dialog|open": () => String.raw`C:\code`,
    set_start_folder: () => {
      saved = { ...SAVED, startFolder: String.raw`C:\code` };
      return saved;
    },
  });

  click(root, "Accounts");
  await settled();
  expect(root.querySelector(".start-folder")?.textContent).toContain(
    "the last folder picked",
  );

  root.querySelector<HTMLButtonElement>(".start-folder button")?.click();
  await settled();
  await settled();

  expect(root.querySelector(".start-folder")?.textContent).toContain(
    String.raw`C:\code`,
  );
});

test("a default account saved after leaving the accounts screen reaches the new tab box", async () => {
  let finishSave: (() => void) | undefined;
  const root = await renderApp(() => SAVED, {
    ...ptyCore().core,
    set_default_account: () =>
      new Promise<Profiles>((resolve) => {
        finishSave = () => {
          resolve({ ...SAVED, defaultAccount: 0 });
        };
      }),
  });
  click(root, "Accounts");
  await settled();

  const picker = root.querySelector<HTMLSelectElement>(".defaults select");
  if (!picker) throw new Error("no account picker");
  picker.value = "0";
  picker.dispatchEvent(new Event("change", { bubbles: true }));
  click(root, "Done");
  await settled();
  finishSave?.();
  await settled();
  await newProject(root);

  expect(pressedAccount(root)).toBe("personal");
});

async function settle(): Promise<void> {
  await settled();
  await settled();
}

async function newProject(root: HTMLElement): Promise<void> {
  click(root, "New project");
  await settle();
}

function newTabBox(root: HTMLElement): Element {
  const box = root.querySelector(".new-tab");
  if (!box) throw new Error("no new tab box");
  return box;
}

function pressedAccount(root: HTMLElement): string | undefined {
  return newTabBox(root)
    .querySelector('.choices button[aria-pressed="true"]')
    ?.textContent.trim();
}

// A core whose sessions get ids 1, 2, 3 in order and whose spawns and kills are recorded.
function ptyCore() {
  const spawned: unknown[] = [];
  const killed: unknown[] = [];
  return {
    spawned,
    killed,
    core: {
      "plugin:dialog|open": () => String.raw`C:\code\alpha`,
      pty_spawn: (args: unknown) => {
        spawned.push(args);
        return spawned.length;
      },
      pty_kill: (args: unknown) => {
        killed.push(field(args, "id"));
      },
    },
  };
}

async function openTabs(
  root: HTMLElement,
  programs: ("Claude Code" | "Shell")[],
  under?: string,
): Promise<void> {
  for (const program of programs) {
    root.querySelector<HTMLButtonElement>(".project .add")?.click();
    await settle();
    if (under) click(newTabBox(root), `Under ${under}`);
    click(newTabBox(root), program);
    await settle();
  }
}

// A project's first tab, opened from the box that New project shows.
async function firstTab(
  root: HTMLElement,
  program: "Claude Code" | "Shell",
): Promise<void> {
  await newProject(root);
  click(newTabBox(root), program);
  await settle();
}

function sidebarTitles(root: HTMLElement): string[] {
  return [...root.querySelectorAll(".tab-row")].map((row) => {
    const indent = row.classList.contains("sub") ? "  " : "";
    return indent + (row.querySelector(".tab-title")?.textContent ?? "");
  });
}

function activeTitle(root: HTMLElement): string | undefined {
  return (
    root.querySelector(".tab-row.active .tab-title")?.textContent ?? undefined
  );
}

test("a new tab starts in its project's folder under the account picked", async () => {
  const pty = ptyCore();
  const root = await renderApp(() => SAVED, pty.core);
  await newProject(root);

  click(newTabBox(root), "personal");
  click(newTabBox(root), "Shell");
  await settle();

  expect(pty.spawned).toHaveLength(1);
  expect(field(pty.spawned[0], "account")).toBe(0);
  expect(field(field(pty.spawned[0], "request"), "cwd")).toBe(
    String.raw`C:\code\alpha`,
  );
  expect(activeTitle(root)).toBe("PowerShell · personal");
});

test("a coder sub-tab lists under its planner", async () => {
  const root = await renderApp(() => SAVED, ptyCore().core);
  await firstTab(root, "Claude Code");

  await openTabs(root, ["Claude Code"], "Claude Code · work");

  expect(sidebarTitles(root)).toEqual([
    "Claude Code · work",
    "  Claude Code · work",
  ]);
  expect(root.querySelector(".tab-row:not(.sub) .tab-sub")?.textContent).toBe(
    "work · 1 coder",
  );
});

test("switching tabs keeps every terminal mounted", async () => {
  const pty = ptyCore();
  const root = await renderApp(() => SAVED, pty.core);
  await firstTab(root, "Shell");
  await openTabs(root, ["Shell", "Shell"]);

  root.querySelector<HTMLButtonElement>(".tab-row .tab-select")?.click();
  await settle();

  expect(root.querySelectorAll(".tab-view")).toHaveLength(3);
  expect(root.querySelectorAll(".tab-view.shown")).toHaveLength(1);
  expect(pty.spawned).toHaveLength(3);
  expect(pty.killed).toEqual([]);
});

test("closing a tab ends only its own session and shows the next tab", async () => {
  const pty = ptyCore();
  const root = await renderApp(() => SAVED, pty.core);
  await firstTab(root, "Shell");
  await openTabs(root, ["Claude Code"]);
  const shellRow = root.querySelector(".tab-row");
  if (!shellRow) throw new Error("no tab row");

  click(shellRow, "Close PowerShell · work");
  await settle();

  expect(pty.killed).toEqual([1]);
  expect(sidebarTitles(root)).toEqual(["Claude Code · work"]);
  expect(activeTitle(root)).toBe("Claude Code · work");
});

function press(key: string, mods: KeyboardEventInit = {}): void {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, ...mods }),
  );
}

test("Ctrl+Tab and Ctrl+Shift+Tab step through the tabs and wrap", async () => {
  const root = await renderApp(() => SAVED, ptyCore().core);
  await firstTab(root, "Shell");
  await openTabs(root, ["Claude Code"]);

  press("Tab", { ctrlKey: true });
  await settle();
  expect(activeTitle(root)).toBe("PowerShell · work");

  press("Tab", { ctrlKey: true, shiftKey: true });
  await settle();
  expect(activeTitle(root)).toBe("Claude Code · work");
});

test("a tab renamed in the sidebar keeps its terminal", async () => {
  const pty = ptyCore();
  const root = await renderApp(() => SAVED, pty.core);
  await firstTab(root, "Shell");

  root
    .querySelector(".tab-select")
    ?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await settle();
  const input = root.querySelector<HTMLInputElement>(".rename");
  if (!input) throw new Error("no rename field");
  input.value = "Build";
  input.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
  );
  await settle();

  expect(sidebarTitles(root)).toEqual(["Build"]);
  expect(pty.spawned).toHaveLength(1);
  expect(pty.killed).toEqual([]);
});

function field(args: unknown, key: string): unknown {
  if (typeof args !== "object" || args === null) return undefined;
  return Object.entries(args).find(([k]) => k === key)?.[1];
}

// A core that keeps one list, raises the revision on every save and can hold an import's reply.
function accountsCore(start: Profiles) {
  let saved = start;
  let holdNextAdd = false;
  const held: (() => void)[] = [];
  const save = (accounts: Account[]) => {
    saved = { ...saved, accounts, revision: saved.revision + 1 };
    // Replies cross IPC as JSON, so every reply holds new objects.
    return structuredClone(saved);
  };
  return {
    profiles: () => saved,
    holdNextAdd: () => {
      holdNextAdd = true;
    },
    release: () => {
      held.splice(0).forEach((reply) => {
        reply();
      });
    },
    core: {
      detect_accounts: () =>
        saved.accounts.some((a) => a.id === 7)
          ? []
          : [{ name: "acc2", configDir: String.raw`C:\h\.claude-acc2` }],
      rename_account: (args: unknown) =>
        save(
          saved.accounts.map((a) =>
            a.id === field(args, "id")
              ? { ...a, name: String(field(args, "name")) }
              : a,
          ),
        ),
      add_account: (args: unknown) => {
        const reply = save([
          ...saved.accounts,
          {
            id: 7,
            name: String(field(args, "name")),
            configDir: String(field(args, "configDir")),
          },
        ]);
        if (!holdNextAdd) return reply;
        holdNextAdd = false;
        return new Promise<Profiles>((resolve) => {
          held.push(() => {
            resolve(reply);
          });
        });
      },
    },
  };
}

const ONE_SAVED: Profiles = { ...SAVED, accounts: SAVED.accounts.slice(0, 1) };

function renameField(root: HTMLElement): HTMLInputElement {
  const input = root.querySelector<HTMLInputElement>(
    '.accounts input[name="name"]',
  );
  if (!input) throw new Error("no rename field");
  return input;
}

function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function accountNames(root: HTMLElement): (string | null)[] {
  return [...root.querySelectorAll(".accounts li strong")].map(
    (s) => s.textContent,
  );
}

test("an import's late reply never undoes a rename saved after it", async () => {
  const fake = accountsCore(ONE_SAVED);
  const root = await renderApp(fake.profiles, fake.core);
  click(root, "Accounts");
  await settled();
  fake.holdNextAdd();

  click(root, "Import");
  await settled();
  click(root, "Rename");
  type(renameField(root), "Main Acc");
  renameField(root).closest("form")?.requestSubmit();
  await settled();
  await settled();
  fake.release();
  await settled();
  await settled();

  expect(accountNames(root)).toEqual(["Main Acc", "acc2"]);
});

test("an open rename keeps what was typed when an import lands", async () => {
  const fake = accountsCore(ONE_SAVED);
  const root = await renderApp(fake.profiles, fake.core);
  click(root, "Accounts");
  await settled();

  click(root, "Rename");
  type(renameField(root), "Main Acc");
  click(root, "Import");
  await settled();
  await settled();

  expect(renameField(root).value).toBe("Main Acc");
});

test("the new tab box offers Claude Code and one Shell button, with no paths", async () => {
  const root = await renderApp(() => SAVED, ptyCore().core);
  await newProject(root);

  const box = newTabBox(root);
  expect(box.querySelector(".primary")?.textContent.trim()).toBe("Claude Code");
  expect(box.querySelector(".secondary")?.textContent.trim()).toBe("Shell");
  expect(box.textContent).not.toContain(".exe");
});

test("the new tab box preselects the default account and shows its details", async () => {
  const root = await renderApp(() => SAVED, ptyCore().core);
  await newProject(root);

  expect(pressedAccount(root)).toBe("work");
  expect(
    newTabBox(root)
      .querySelector('.choices button[aria-pressed="true"]')
      ?.getAttribute("title"),
  ).toBe("user@example.com · Pro");
});

test("Claude Code cannot be opened when it is not installed", async () => {
  const root = await renderApp(() => SAVED, {
    ...ptyCore().core,
    list_programs: () =>
      CHOICES.map((c) => (c.id === "claude" ? { ...c, program: null } : c)),
  });
  await newProject(root);

  expect(
    newTabBox(root).querySelector<HTMLButtonElement>(".primary")?.disabled,
  ).toBe(true);
  expect(newTabBox(root).textContent).toContain("Claude Code isn't installed.");
});

test("an unreadable accounts file offers to start fresh", async () => {
  const root = await renderApp(
    () => {
      throw new Error("the accounts file is not valid");
    },
    { reset_profiles: () => EMPTY },
  );

  expect(root.textContent).toContain("the accounts file is not valid");
  const startFresh = [...root.querySelectorAll("button")].find(
    (b) => b.textContent === "Start fresh",
  );
  startFresh?.click();
  await settled();
  await settled();

  expect(root.querySelector("h1")?.textContent).toBe("Set up Cerberus");
});
