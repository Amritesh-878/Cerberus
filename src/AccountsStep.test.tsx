import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import { AccountsStep } from "./AccountsStep";
import type { ProgramChoice } from "./pty";
import { createStatuses } from "./statuses";
import type { Tab } from "./tab";

const EMPTY: Profiles = {
  accounts: [],
  defaultAccount: null,
  shell: null,
  startFolder: null,
  revision: 0,
};

const CLAUDE: ProgramChoice = {
  id: "claude",
  label: "Claude Code",
  program: String.raw`C:\bin\claude.exe`,
  args: [],
  pastesImages: true,
};

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  clearMocks();
});

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function field(args: unknown, key: string): string {
  if (typeof args !== "object" || args === null) return "";
  const value: unknown = Object.entries(args).find(([k]) => k === key)?.[1];
  return typeof value === "string" ? value : "";
}

function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function button(root: HTMLElement, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`no ${text} button`);
  return found;
}

interface Rendered {
  root: HTMLElement;
  name: HTMLInputElement;
  form: HTMLFormElement;
  added: unknown[];
  opened: Tab[];
  folder: () => string | undefined;
}

async function renderStep(
  options: {
    profiles?: Profiles;
    picked?: string | null;
    state?: "loggedIn" | "loggedOut";
  } = {},
): Promise<Rendered> {
  const added: unknown[] = [];
  const opened: Tab[] = [];
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "suggest_config_dir":
        return String.raw`C:\h\.claude-` + field(args, "name");
      case "add_account":
        added.push(args);
        return {
          ...EMPTY,
          accounts: [
            {
              id: 5,
              name: field(args, "name"),
              configDir: field(args, "configDir"),
            },
          ],
          defaultAccount: 5,
        };
      case "detect_accounts":
        return [];
      case "account_status":
        return { state: options.state ?? "loggedIn", summary: "" };
      case "plugin:dialog|open":
        return options.picked ?? null;
      default:
        return null;
    }
  });
  const root = document.createElement("div");
  document.body.append(root);
  const statuses = createStatuses();
  const profiles = options.profiles ?? EMPTY;
  statuses.refresh(profiles.accounts.map((a) => a.id));
  dispose = render(
    () => (
      <AccountsStep
        profiles={profiles}
        programs={[CLAUDE]}
        statuses={statuses}
        onProfiles={() => undefined}
        onOpen={(tab) => opened.push(tab)}
      />
    ),
    root,
  );
  await settled();
  const name = root.querySelector<HTMLInputElement>(".add-account input");
  const form = root.querySelector<HTMLFormElement>(".add-account");
  if (!name || !form) throw new Error("no add form");
  const folder = () =>
    root.querySelector(".folder .program")?.textContent ?? undefined;
  return { root, name, form, added, opened, folder };
}

test("the add form asks only for a name", async () => {
  const { root } = await renderStep();

  expect(root.querySelectorAll(".add-account input")).toHaveLength(1);
});

test("the folder follows the name until one is picked", async () => {
  const { root, name, folder } = await renderStep({
    picked: String.raw`D:\mine`,
  });

  type(name, "work");
  await settled();
  expect(folder()).toBe(String.raw`C:\h\.claude-work`);

  button(root, "Change…").click();
  await settled();
  expect(folder()).toBe(String.raw`D:\mine`);

  type(name, "work two");
  await settled();
  expect(folder()).toBe(String.raw`D:\mine`);
});

test("cancelling the folder picker keeps the suggested folder", async () => {
  const { root, name, folder } = await renderStep({ picked: null });
  type(name, "work");
  await settled();

  button(root, "Change…").click();
  await settled();

  expect(folder()).toBe(String.raw`C:\h\.claude-work`);
});

test("submitting before the suggestion arrives still adds with a folder", async () => {
  const { name, form, added } = await renderStep();

  type(name, "work");
  form.requestSubmit();
  await settled();
  await settled();

  expect(added).toEqual([
    { name: "work", configDir: String.raw`C:\h\.claude-work` },
  ]);
});

test("renaming before a new suggestion arrives never adds the old folder", async () => {
  const { name, form, added, folder } = await renderStep();
  type(name, "work");
  await settled();

  type(name, "personal");
  expect(folder()).toBeUndefined();
  form.requestSubmit();
  await settled();
  await settled();

  expect(added).toEqual([
    { name: "personal", configDir: String.raw`C:\h\.claude-personal` },
  ]);
});

test("adding an account opens a sign-in tab under it", async () => {
  const { name, form, opened } = await renderStep();
  type(name, "work");
  await settled();

  form.requestSubmit();
  await settled();
  await settled();

  expect(opened).toHaveLength(1);
  expect(opened[0]?.signIn).toBe(true);
  expect(opened[0]?.account).toEqual({ id: 5, name: "work" });
});

const ONE: Profiles = {
  ...EMPTY,
  accounts: [{ id: 0, name: "personal", configDir: String.raw`C:\h\.claude` }],
  defaultAccount: 0,
};

test("sign in is offered only to a signed-out account", async () => {
  const signedIn = await renderStep({ profiles: ONE, state: "loggedIn" });
  await settled();
  expect(signedIn.root.textContent).not.toContain("Sign in");
  dispose?.();

  const signedOut = await renderStep({ profiles: ONE, state: "loggedOut" });
  await settled();
  button(signedOut.root, "Sign in").click();

  expect(signedOut.opened[0]?.signIn).toBe(true);
});

test("a row being renamed shows only Save and Cancel", async () => {
  const { root } = await renderStep({ profiles: ONE });
  const row = root.querySelector(".accounts li");
  if (!(row instanceof HTMLElement)) throw new Error("no account row");
  const labels = () =>
    [...row.querySelectorAll("button")].map((b) => b.textContent);

  button(row, "Rename").click();
  expect(labels()).toEqual(["Save", "Cancel"]);

  button(row, "Cancel").click();
  expect(labels()).toEqual(["Rename", "Remove"]);
});
