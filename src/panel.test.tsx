import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";

import type { PanelPrefs, Profiles } from "./accounts";
import {
  activeTitle,
  button,
  cleanup,
  click,
  emit,
  field,
  open,
  press,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import { type Changes, type Skill, skillCommand } from "./panelData";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const ALPHA = String.raw`C:\code\alpha`;
const NOTES = String.raw`C:\code\notes`;

const TREE: SavedTree = {
  projects: [
    { id: 1, name: "alpha", folder: ALPHA },
    { id: 2, name: "notes", folder: NOTES },
  ],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: ALPHA,
      account: 1,
    },
    {
      id: 4,
      projectId: 2,
      parentId: null,
      title: "notes shell",
      program: "powershell",
      cwd: NOTES,
      account: 0,
    },
  ],
  nextId: 5,
};

const REPO: Changes = {
  kind: "repo",
  name: "alpha",
  root: ALPHA,
  branch: "feature",
  commit: null,
  base: "origin/main",
  added: 736,
  removed: 2,
  gh: true,
  vscode: true,
};

// In the core's use order: most used first, then by name.
const SKILLS: Skill[] = [
  { name: "verify", description: "Run the gate", scope: "project", uses: 9 },
  { name: "release", description: null, scope: "account", uses: 2 },
  { name: "notes", description: "Draft notes", scope: "account", uses: 0 },
];

function withPanel(panel: Partial<PanelPrefs>): Profiles {
  return {
    ...SAVED,
    panel: {
      open: false,
      width: null,
      collapsed: [],
      expanded: [],
      favourites: {},
      ...panel,
    },
  };
}

/** A core whose panel reads are recorded, answering per folder. */
function panelCore(start: Profiles) {
  const pty = ptyCore();
  const calls: { cmd: string; args: unknown }[] = [];
  const saved: PanelPrefs[] = [];
  let profiles = start;
  const record = (cmd: string, answer: (args: unknown) => unknown) => ({
    [cmd]: (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    },
  });
  return {
    pty,
    calls,
    saved,
    profiles: () => profiles,
    reads: () => calls.filter((c) => c.cmd.startsWith("panel_")),
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      set_panel: (args: unknown) => {
        const panel = field(args, "panel") as PanelPrefs;
        saved.push(panel);
        profiles = { ...profiles, panel, revision: profiles.revision + 1 };
        return profiles;
      },
      ...record("panel_changes", (args) =>
        field(args, "folder") === ALPHA ? REPO : { kind: "notRepo" },
      ),
      ...record("panel_prs", () => [
        {
          number: 21,
          title: "Add the export",
          state: "open",
          key: "example/alpha#21",
          updated: "u1",
        },
      ]),
      ...record("panel_connectors", (args) => ({
        servers:
          field(args, "account") === 1
            ? [{ name: "browser", kind: "stdio", scope: "user" }]
            : [],
        unreadable: [],
      })),
      ...record("panel_skills", () => SKILLS),
      ...record("panel_open_pr", () => null),
      ...record("panel_open_folder", () => null),
      ...record("panel_open_vscode", () => null),
    },
  };
}

function panel(root: HTMLElement): HTMLElement | null {
  return root.querySelector<HTMLElement>(".panel");
}

function card(root: HTMLElement, title: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".card")].find(
    (c) => c.querySelector(".card-title")?.textContent === title,
  );
  if (!found) throw new Error(`no ${title} card`);
  return found;
}

test("a closed panel reads nothing, and its button opens it and saves that", async () => {
  const core = panelCore(withPanel({}));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  expect(panel(root)).toBeNull();
  expect(core.reads()).toEqual([]);

  click(root, "Right panel");
  await settle();

  expect(panel(root)).not.toBeNull();
  expect(core.saved.at(-1)?.open).toBe(true);
  expect(core.reads().map((c) => c.cmd)).toEqual(
    expect.arrayContaining([
      "panel_changes",
      "panel_prs",
      "panel_connectors",
      "panel_skills",
    ]),
  );
  expect(button(root, "Right panel").getAttribute("aria-pressed")).toBe("true");
});

test("Ctrl+Shift+B closes and opens the panel, and closing stops the reads", async () => {
  const core = panelCore(withPanel({ open: true }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");
  expect(panel(root)).not.toBeNull();

  press("B", { ctrlKey: true, shiftKey: true });
  await settle();
  const before = core.reads().length;
  await open(root, "notes shell");

  expect(panel(root)).toBeNull();
  expect(core.saved.at(-1)?.open).toBe(false);
  expect(core.reads()).toHaveLength(before);

  press("B", { ctrlKey: true, shiftKey: true });
  await settle();
  expect(panel(root)).not.toBeNull();
});

test("the cards show the shown tab and switch with it", async () => {
  const core = panelCore(withPanel({ open: true, expanded: ["connectors"] }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  const changes = card(root, "Changes").textContent;
  expect(changes).toContain("+736");
  expect(changes).toContain("−2");
  expect(changes).toContain("Add the export");
  expect(changes).toContain("alpha #21");
  expect(card(root, "Environment").textContent).toContain("work");
  expect(card(root, "Environment").textContent).toContain("feature");
  expect(card(root, "Connectors").textContent).toContain("1 configured");

  await open(root, "notes shell");

  expect(card(root, "Changes").textContent).toBe("ChangesNot a git repository");
  expect(card(root, "Environment").textContent).toContain("personal");
  expect(card(root, "Environment").textContent).not.toContain("feature");
  expect(card(root, "Connectors").textContent).toContain("None configured");
  const last = core.calls.filter((c) => c.cmd === "panel_connectors").at(-1);
  expect(field(last?.args, "folder")).toBe(NOTES);
  expect(field(last?.args, "account")).toBe(0);
  expect(field(last?.args, "tab")).toBe(4);
});

test("a card folds from its title and the folded set is saved", async () => {
  const core = panelCore(withPanel({ open: true }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  card(root, "Skills").querySelector<HTMLButtonElement>(".card-title")?.click();
  await settle();

  expect(card(root, "Skills").querySelector(".card-body")).toBeNull();
  expect(core.saved.at(-1)?.collapsed).toEqual(["skills"]);
  expect(core.profiles().panel.collapsed).toEqual(["skills"]);
});

test("a saved folded card and width come back on start", async () => {
  const core = panelCore(
    withPanel({ open: true, width: 360, collapsed: ["skills"] }),
  );
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  expect(panel(root)?.style.getPropertyValue("--panel-width")).toBe("360px");
  expect(card(root, "Skills").querySelector(".card-body")).toBeNull();
  expect(card(root, "Changes").querySelector(".card-body")).not.toBeNull();
});

test("the panel edge resizes from the left and double-click resets", async () => {
  Object.defineProperty(window, "innerWidth", {
    value: 1280,
    configurable: true,
  });
  const core = panelCore(withPanel({ open: true, width: 300 }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");
  const edge = root.querySelector(".panel-edge");

  edge?.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }),
  );
  await settle();
  expect(panel(root)?.style.getPropertyValue("--panel-width")).toBe("316px");

  edge?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await settle();

  expect(core.saved.map((p) => p.width)).toEqual([316, 300]);
});

test("Create PR opens a shell in the folder with the command typed, never sent", async () => {
  const core = panelCore(withPanel({ open: true }));
  const writes: string[] = [];
  const root = await renderApp(core.profiles, {
    ...core.core,
    pty_write: (args: unknown) => {
      writes.push(String(field(args, "data")));
    },
  });
  await open(root, "planner");

  click(card(root, "Changes"), "Create PR");
  await settle();
  // The command is typed once the shell has drawn its prompt.
  emit(
    core.pty.spawned,
    core.pty.spawned.length - 1,
    new TextEncoder().encode("PS> ").buffer,
  );
  await settle();
  await settle();

  expect(activeTitle(root)).toBe("Create PR");
  const spawn = core.pty.spawned.at(-1);
  expect(field(field(spawn, "request"), "cwd")).toBe(ALPHA);
  expect(field(field(spawn, "request"), "program")).toBe(
    String.raw`C:\ps\powershell.exe`,
  );
  expect(writes).toEqual(["gh pr create --fill"]);
});

test("the PR row opens through the core, which holds the URL", async () => {
  const core = panelCore(withPanel({ open: true }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  click(card(root, "Changes"), "More for #21");
  await settle();
  click(card(root, "Changes"), "Open on GitHub");
  await settle();

  const opened = core.calls.filter((c) => c.cmd === "panel_open_pr");
  expect(
    opened.map((c) => [field(c.args, "folder"), field(c.args, "number")]),
  ).toEqual([[ALPHA, 21]]);
});

function skillNames(root: HTMLElement): string[] {
  return [...card(root, "Skills").querySelectorAll(".skill-row .row-main")].map(
    (row) => row.textContent,
  );
}

function skillRow(root: HTMLElement, name: string): HTMLElement {
  const found = [
    ...card(root, "Skills").querySelectorAll<HTMLElement>(".skill-row"),
  ].find((row) => row.querySelector(".row-main")?.textContent === name);
  if (!found) throw new Error(`no ${name} skill`);
  return found;
}

function insertButton(row: HTMLElement): HTMLElement {
  const found = row.querySelector<HTMLElement>(".skill-main");
  if (!found) throw new Error("no insert button");
  return found;
}

function shiftClick(row: HTMLElement): void {
  insertButton(row).dispatchEvent(
    new MouseEvent("click", { bubbles: true, shiftKey: true }),
  );
}

/** A panel core whose terminal writes are recorded. */
function typingCore() {
  const core = panelCore(withPanel({ open: true }));
  const writes: string[] = [];
  return {
    core,
    writes,
    ipc: {
      ...core.core,
      pty_write: (args: unknown) => {
        writes.push(String(field(args, "data")));
      },
    },
  };
}

test("Connectors sits below Skills, folded until opened, and opening it is kept", async () => {
  const core = panelCore(withPanel({ open: true }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  const titles = [...root.querySelectorAll(".card-title")].map(
    (t) => t.textContent,
  );
  expect(titles).toEqual([
    "Environment",
    "Context",
    "Outputs",
    "Background tasks",
    "Usage",
    "Changes",
    "Skills",
    "Connectors",
  ]);
  expect(card(root, "Connectors").querySelector(".card-body")).toBeNull();

  card(root, "Connectors")
    .querySelector<HTMLButtonElement>(".card-title")
    ?.click();
  await settle();

  expect(card(root, "Connectors").textContent).toContain(
    "MCP servers this chat loads",
  );
  expect(core.saved.at(-1)?.expanded).toEqual(["connectors"]);
  expect(core.saved.at(-1)?.collapsed).toEqual([]);

  await cleanup();
  const again = await renderApp(core.profiles, core.core);
  await open(again, "planner");
  expect(card(again, "Connectors").querySelector(".card-body")).not.toBeNull();
});

test("skills come in use order, and a star pins one to the top for that account", async () => {
  const core = panelCore(withPanel({ open: true }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");
  expect(skillNames(root)).toEqual(["verify", "release", "notes"]);

  click(skillRow(root, "notes"), "Favourite notes");
  await settle();

  expect(skillNames(root)).toEqual(["notes", "verify", "release"]);
  expect(core.saved.at(-1)?.favourites).toEqual({ "1": ["notes"] });

  await open(root, "notes shell");
  expect(skillNames(root)).toEqual(["verify", "release", "notes"]);

  await cleanup();
  const again = await renderApp(core.profiles, core.core);
  await open(again, "planner");
  expect(skillNames(again)).toEqual(["notes", "verify", "release"]);
  expect(
    button(skillRow(again, "notes"), "Favourite notes").getAttribute(
      "aria-pressed",
    ),
  ).toBe("true");
});

test("starred skills keep their use order above the rest", async () => {
  const core = panelCore(
    withPanel({ open: true, favourites: { "1": ["notes", "verify"] } }),
  );
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  expect(skillNames(root)).toEqual(["verify", "notes", "release"]);
});

test("shift-click or Shift+Enter types the skill into the chat without Enter, and a plain click types nothing", async () => {
  const { core, writes, ipc } = typingCore();
  const root = await renderApp(core.profiles, ipc);
  await open(root, "planner");

  insertButton(skillRow(root, "release")).click();
  click(skillRow(root, "release"), "Favourite release");
  shiftClick(skillRow(root, "verify"));
  insertButton(skillRow(root, "notes")).dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
  );
  insertButton(skillRow(root, "notes")).dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      shiftKey: true,
      bubbles: true,
    }),
  );
  await settle();

  expect(writes).toEqual(["/verify", "/notes"]);
  expect(skillRow(root, "verify").title).toContain(
    "Shift-click or Shift+Enter types /verify into the chat, for you to send",
  );
  expect(skillRow(root, "verify").title).toContain("Used 9 times");
});

test("shift-click types nothing into a shell tab, and the tooltip says where it works", async () => {
  const { core, writes, ipc } = typingCore();
  const root = await renderApp(core.profiles, ipc);
  await open(root, "notes shell");

  shiftClick(skillRow(root, "verify"));
  await settle();

  expect(writes).toEqual([]);
  expect(skillRow(root, "verify").title).toContain(
    "Shift-click types /verify in Claude Code tabs",
  );
});

test("a skill name never types a control character, so never an Enter", () => {
  expect(skillCommand("verify")).toBe("/verify");
  expect(skillCommand("bad\rname\n\u001b[A\u009b")).toBe("/badname[A");
});

test("Open in VS Code asks the core for the folder, and hides without VS Code", async () => {
  const core = panelCore(withPanel({ open: true }));
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  click(card(root, "Changes"), "Open in VS Code");
  await settle();
  expect(
    core.calls
      .filter((c) => c.cmd === "panel_open_vscode")
      .map((c) => field(c.args, "folder")),
  ).toEqual([ALPHA]);

  await cleanup();
  const without = panelCore(withPanel({ open: true }));
  const again = await renderApp(without.profiles, {
    ...without.core,
    panel_changes: () => ({ ...REPO, vscode: false }),
  });
  await open(again, "planner");
  expect(card(again, "Changes").textContent).toContain("+736");
  expect(() => button(card(again, "Changes"), "Open in VS Code")).toThrow();
});

test("plugin and claude.ai skills are marked, and Connectors names Cerberus's own server and the claude.ai gap", async () => {
  const core = panelCore(withPanel({ open: true, expanded: ["connectors"] }));
  const root = await renderApp(core.profiles, {
    ...core.core,
    panel_skills: () => [
      {
        name: "tools:audit",
        description: "Check a page",
        scope: "plugin",
        uses: 3,
      },
      {
        name: "anthropic-skills:pdf",
        description: null,
        scope: "synced",
        uses: 1,
      },
      ...SKILLS.slice(0, 1),
    ],
    panel_connectors: () => ({
      servers: [
        { name: "cerberus", kind: "stdio", scope: "app" },
        { name: "tools:browser", kind: "stdio", scope: "plugin" },
      ],
      unreadable: [],
    }),
  });
  await open(root, "planner");

  const detail = (name: string) =>
    skillRow(root, name).querySelector(".row-detail")?.textContent;
  expect(detail("tools:audit")).toBe("pluginCheck a page");
  expect(detail("anthropic-skills:pdf")).toBe("claude.ai");
  expect(detail("verify")).toBe("Run the gate");
  const connectors = card(root, "Connectors").textContent;
  expect(connectors).toContain("cerberusstdio · Cerberus");
  expect(connectors).toContain("tools:browserstdio · plugin");
  expect(connectors).toContain("2 configured");
  expect(connectors).toContain(
    "Connectors added on claude.ai are not listed here",
  );
});

test("a bridge report re-reads the shown tab's connectors, so cerberus comes and goes with its link", async () => {
  const core = panelCore(withPanel({ open: true, expanded: ["connectors"] }));
  let linked = false;
  const root = await renderApp(core.profiles, {
    ...core.core,
    panel_connectors: () => ({
      servers: linked
        ? [{ name: "cerberus", kind: "stdio", scope: "app" }]
        : [],
      unreadable: [],
    }),
  });
  await open(root, "planner");
  expect(card(root, "Connectors").textContent).not.toContain("cerberus");

  linked = true;
  await emitEvent("bridge", { pending: [], notes: [] });
  await settle();
  expect(card(root, "Connectors").textContent).toContain("stdio · Cerberus");

  linked = false;
  await emitEvent("bridge", { pending: [], notes: [] });
  await settle();
  expect(card(root, "Connectors").textContent).not.toContain("cerberus");
});
