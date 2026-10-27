import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import { AccountsStep } from "./AccountsStep";
import { createStatuses } from "./statuses";

const EMPTY: Profiles = {
  accounts: [],
  defaultAccount: null,
  defaultProgram: null,
  defaultCwd: null,
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

function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

async function renderStep(): Promise<{
  name: HTMLInputElement;
  folder: HTMLInputElement;
  form: HTMLFormElement;
  added: unknown[];
}> {
  const added: unknown[] = [];
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "suggest_config_dir":
        return String.raw`C:\h\.claude-suggested`;
      case "add_account":
        added.push(args);
        return EMPTY;
      case "detect_accounts":
        return [];
      default:
        return null;
    }
  });
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(
    () => (
      <AccountsStep
        profiles={EMPTY}
        programs={[]}
        statuses={createStatuses()}
        onProfiles={() => undefined}
        onOpen={() => undefined}
      />
    ),
    root,
  );
  await settled();
  const [name, folder] = [
    ...root.querySelectorAll<HTMLInputElement>(".add-account input"),
  ];
  const form = root.querySelector<HTMLFormElement>(".add-account");
  if (!name || !folder || !form) throw new Error("no add form");
  return { name, folder, form, added };
}

test("the folder follows the name until it is typed by hand", async () => {
  const { name, folder } = await renderStep();

  type(name, "work");
  await settled();
  expect(folder.value).toBe(String.raw`C:\h\.claude-suggested`);

  type(folder, String.raw`D:\mine`);
  type(name, "work two");
  await settled();
  expect(folder.value).toBe(String.raw`D:\mine`);
});

test("submitting before the suggestion arrives still adds with a folder", async () => {
  const { name, form, added } = await renderStep();

  type(name, "work");
  form.requestSubmit();
  await settled();
  await settled();

  expect(added).toEqual([
    { name: "work", configDir: String.raw`C:\h\.claude-suggested` },
  ]);
});
