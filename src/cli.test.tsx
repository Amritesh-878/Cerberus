import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { CliInstall } from "./AccountsScreen";
import { linksCli } from "./cli";

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

/** A core whose install answers with `install` and whose state line is `state`. */
function core(
  install: () => unknown,
  state: () => unknown = () => "The `cerberus` command isn't installed.",
): string[] {
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    return cmd === "cli_state" ? state() : install();
  });
  return calls;
}

function install(root: HTMLElement): void {
  const button = root.querySelector("button");
  if (!button) throw new Error("no install button");
  button.click();
}

test("the install button shows the core's message and the line to add", async () => {
  const calls = core(() => ({
    message: "Linked ~/.local/bin/cerberus to this app. Add this line:",
    pathLine: 'export PATH="$HOME/.local/bin:$PATH"',
  }));
  const root = mount();
  await settled();

  install(root);
  await settled();

  expect(calls).toEqual(["cli_state", "install_cli", "cli_state"]);
  expect(root.querySelector(".cli-result")?.textContent).toContain("Linked");
  expect(root.querySelector("pre")?.textContent).toBe(
    'export PATH="$HOME/.local/bin:$PATH"',
  );
});

test("no line shows when the folder is already on PATH", async () => {
  core(() => ({ message: "Linked.", pathLine: null }));
  const root = mount();

  install(root);
  await settled();

  expect(root.querySelector(".cli-result")?.textContent).toBe("Linked.");
  expect(root.querySelector("pre")).toBeNull();
});

test("the button waits for one install to finish before it takes another", async () => {
  let finish: (linked: unknown) => void = () => undefined;
  let calls = 0;
  core(() => {
    calls += 1;
    return new Promise((resolve) => {
      finish = resolve;
    });
  });
  const root = mount();
  const button = root.querySelector("button");

  install(root);
  install(root);
  await settled();

  expect(calls).toBe(1);
  expect(button?.disabled).toBe(true);
  finish({ message: "Linked.", pathLine: null });
  await settled();
  expect(button?.disabled).toBe(false);
});

test("a refusal is shown as an error", async () => {
  core(() => {
    throw new Error("move it to Applications first");
  });
  const root = mount();

  install(root);
  await settled();

  expect(root.querySelector(".error")?.textContent).toContain(
    "move it to Applications first",
  );
});

test("the state line shows what the command runs, and is read again after an install", async () => {
  let state = "The `cerberus` command isn't installed.";
  core(
    () => {
      state = "`cerberus` runs this AppImage.";
      return { message: "Wrote it.", pathLine: null };
    },
    () => state,
  );
  const root = mount();
  await settled();
  expect(root.querySelector(".cli-state")?.textContent).toBe(
    "The `cerberus` command isn't installed.",
  );

  install(root);
  await settled();
  await settled();

  expect(root.querySelector(".cli-state")?.textContent).toBe(
    "`cerberus` runs this AppImage.",
  );
});

test("macOS and Linux offer the command link; Windows' installer already puts it on PATH", () => {
  expect(linksCli("mac")).toBe(true);
  expect(linksCli("linux")).toBe(true);
  expect(linksCli("other")).toBe(false);
});
