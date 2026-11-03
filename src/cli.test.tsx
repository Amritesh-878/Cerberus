import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { CliInstall } from "./AccountsScreen";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  clearMocks();
});

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function mount(): HTMLElement {
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(() => <CliInstall />, root);
  return root;
}

function install(root: HTMLElement): void {
  const button = root.querySelector("button");
  if (!button) throw new Error("no install button");
  button.click();
}

test("the install button shows the core's message and the line to add", async () => {
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    return {
      message: "Linked ~/.local/bin/cerberus to this app. Add this line:",
      pathLine: 'export PATH="$HOME/.local/bin:$PATH"',
    };
  });
  const root = mount();

  install(root);
  await settled();

  expect(calls).toEqual(["install_cli"]);
  expect(root.querySelector("p")?.textContent).toContain("Linked");
  expect(root.querySelector("pre")?.textContent).toBe(
    'export PATH="$HOME/.local/bin:$PATH"',
  );
});

test("no line shows when the folder is already on PATH", async () => {
  mockIPC(() => ({ message: "Linked.", pathLine: null }));
  const root = mount();

  install(root);
  await settled();

  expect(root.querySelector("p")?.textContent).toBe("Linked.");
  expect(root.querySelector("pre")).toBeNull();
});

test("a refusal is shown as an error", async () => {
  mockIPC(() => {
    throw new Error("move it to Applications first");
  });
  const root = mount();

  install(root);
  await settled();

  expect(root.querySelector(".error")?.textContent).toContain(
    "move it to Applications first",
  );
});
