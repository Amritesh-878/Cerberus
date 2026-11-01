import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render } from "solid-js/web";

import type { Profiles } from "./accounts";
import { App } from "./App";
import type { ProgramChoice } from "./pty";

let dispose: (() => void) | undefined;

/** Call from `afterEach`, so each test starts with no app and no IPC mock. */
export async function cleanup(): Promise<void> {
  dispose?.();
  document.body.innerHTML = "";
  // Event listeners unregister a tick after disposal and need the mocks still in place.
  await settled();
  clearMocks();
}

export const CHOICES: ProgramChoice[] = [
  {
    id: "claude",
    label: "Claude Code",
    program: String.raw`C:\bin\claude.exe`,
    args: [],
    pastesImages: true,
    isShell: false,
  },
  {
    id: "powershell",
    label: "PowerShell",
    program: String.raw`C:\ps\powershell.exe`,
    args: ["-NoLogo"],
    pastesImages: false,
    isShell: true,
  },
];

export const EMPTY: Profiles = {
  accounts: [],
  defaultAccount: null,
  shell: null,
  startFolder: null,
  revision: 0,
};

export const SAVED: Profiles = {
  accounts: [
    { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 1, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 1,
  shell: null,
  startFolder: null,
  revision: 0,
};

export async function settled(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

export async function renderApp(
  profiles: () => Profiles,
  core: Record<string, (args: unknown) => unknown> = {},
): Promise<HTMLElement> {
  mockIPC(
    (cmd, args) => {
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
        case "tree_load":
          return {
            tree: { projects: [], tabs: [], nextId: 1 },
            notice: null,
          };
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(() => <App />, root);
  await settled();
  await settled();
  return root;
}

export function button(root: ParentNode, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll("button")].find(
    (b) =>
      b.textContent.trim() === text || b.getAttribute("aria-label") === text,
  );
  if (!found) throw new Error(`no ${text} button`);
  return found;
}

export function click(root: ParentNode, text: string): void {
  button(root, text).click();
}

export async function settle(): Promise<void> {
  await settled();
  await settled();
}

// A core whose sessions get ids 1, 2, 3 in order and whose spawns and kills are recorded.
export function ptyCore() {
  const spawned: unknown[] = [];
  const killed: unknown[] = [];
  return {
    spawned,
    killed,
    core: {
      "plugin:dialog|open": () => String.raw`C:\code\alpha`,
      pty_spawn: (args: unknown) => {
        spawned.push(args);
        return { id: spawned.length, resume: "new" };
      },
      pty_kill: (args: unknown) => {
        killed.push(field(args, "id"));
      },
    },
  };
}

export function sidebarTitles(root: HTMLElement): string[] {
  return [...root.querySelectorAll(".tab-row")].map((row) => {
    const indent = row.classList.contains("sub") ? "  " : "";
    return indent + (row.querySelector(".tab-title")?.textContent ?? "");
  });
}

export function activeTitle(root: HTMLElement): string | undefined {
  return (
    root.querySelector(".tab-row.active .tab-title")?.textContent ?? undefined
  );
}

export function press(key: string, mods: KeyboardEventInit = {}): void {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, ...mods }),
  );
}

export function field(args: unknown, key: string): unknown {
  if (typeof args !== "object" || args === null) return undefined;
  return Object.entries(args).find(([k]) => k === key)?.[1];
}
