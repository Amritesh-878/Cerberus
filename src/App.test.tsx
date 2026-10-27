import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { App } from "./App";
import type { ProgramChoice } from "./pty";

const CHOICES: ProgramChoice[] = [
  { id: "claude", label: "Claude Code", program: null, args: [] },
  {
    id: "powershell",
    label: "PowerShell",
    program: String.raw`C:\ps\powershell.exe`,
    args: ["-NoLogo"],
  },
];

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  clearMocks();
});

async function renderApp(): Promise<HTMLElement> {
  mockIPC((cmd) => (cmd === "list_programs" ? CHOICES : null));
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(() => <App />, root);
  await new Promise((resolve) => setTimeout(resolve, 0));
  return root;
}

test("the launcher lists each program from the core", async () => {
  const root = await renderApp();

  const items = [...root.querySelectorAll("li")].map((li) => li.textContent);
  expect(items).toEqual([
    "Claude Codenot installed",
    String.raw`PowerShellC:\ps\powershell.exe`,
  ]);
});

test("a program that is not installed cannot be launched", async () => {
  const root = await renderApp();

  const buttons = [...root.querySelectorAll("button")];
  expect(buttons.map((b) => b.disabled)).toEqual([true, false]);
});
