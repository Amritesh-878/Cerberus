import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
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
};

const SAVED: Profiles = {
  accounts: [
    { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 1, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 1,
  lastProgram: "powershell",
  startFolder: null,
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
  core: Record<string, () => unknown> = {},
): Promise<HTMLElement> {
  mockIPC((cmd) => {
    if (cmd in core) return core[cmd]?.();
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
