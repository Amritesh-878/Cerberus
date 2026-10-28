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
    program: null,
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
  lastProgram: null,
  startFolder: null,
  revision: 0,
};

const SAVED: Profiles = {
  accounts: [
    { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 1, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 1,
  lastProgram: "powershell",
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
  const root = await renderApp(() => EMPTY);

  expect(root.querySelector("h1")?.textContent).toBe("Set up Cerberus");
  expect(root.textContent).toContain("Claude Code isn't installed yet");
});

function click(root: HTMLElement, text: string): void {
  const found = [...root.querySelectorAll("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`no ${text} button`);
  found.click();
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

test("the accounts screen sets the start folder, Home by default", async () => {
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    "plugin:dialog|open": () => String.raw`C:\code`,
    set_start_folder: () => {
      saved = { ...SAVED, startFolder: String.raw`C:\code` };
      return saved;
    },
  });

  click(root, "Accounts…");
  await settled();
  expect(root.querySelector(".start-folder")?.textContent).toContain("Home");

  root.querySelector<HTMLButtonElement>(".start-folder button")?.click();
  await settled();
  await settled();

  expect(root.querySelector(".start-folder")?.textContent).toContain(
    String.raw`C:\code`,
  );
});

test("a default account saved after leaving the accounts screen reaches the launcher", async () => {
  let finishSave: (() => void) | undefined;
  const root = await renderApp(() => SAVED, {
    set_default_account: () =>
      new Promise<Profiles>((resolve) => {
        finishSave = () => {
          resolve({ ...SAVED, defaultAccount: 0 });
        };
      }),
  });
  click(root, "Accounts…");
  await settled();

  const picker = root.querySelector<HTMLSelectElement>(".defaults select");
  if (!picker) throw new Error("no account picker");
  picker.value = "0";
  picker.dispatchEvent(new Event("change", { bubbles: true }));
  click(root, "Back to launcher");
  await settled();
  finishSave?.();
  await settled();
  await settled();

  expect(
    root.querySelector<HTMLSelectElement>(".account-picker select")?.value,
  ).toBe("0");
});

test("opening a program keeps the account picked in the launcher", async () => {
  const root = await renderApp(() => SAVED, {
    remember_program: () => ({ ...SAVED, lastProgram: "powershell" }),
  });
  const picker = root.querySelector<HTMLSelectElement>(
    ".account-picker select",
  );
  if (!picker) throw new Error("no account picker");
  picker.value = "0";
  picker.dispatchEvent(new Event("change", { bubbles: true }));

  click(root, "PowerShell");
  await settled();
  await settled();

  expect(picker.value).toBe("0");
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
  click(root, "Accounts…");
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
  click(root, "Accounts…");
  await settled();

  click(root, "Rename");
  type(renameField(root), "Main Acc");
  click(root, "Import");
  await settled();
  await settled();

  expect(renameField(root).value).toBe("Main Acc");
});

test("saved accounts open the launcher with each program from the core", async () => {
  const root = await renderApp(() => SAVED);

  const items = [...root.querySelectorAll("li")].map((li) => li.textContent);
  expect(items).toEqual([
    "Claude Codenot installed",
    String.raw`PowerShellC:\ps\powershell.exe`,
  ]);
});

test("the launcher preselects the default account and shows its details", async () => {
  const root = await renderApp(() => SAVED);

  expect(root.querySelector("select")?.value).toBe("1");
  expect(root.querySelector(".account-picker")?.textContent).toContain(
    "user@example.com · Pro",
  );
});

test("a program that is not installed cannot be launched", async () => {
  const root = await renderApp(() => SAVED);

  const buttons = [...root.querySelectorAll("li button")];
  expect(buttons.map((b) => (b as HTMLButtonElement).disabled)).toEqual([
    true,
    false,
  ]);
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
