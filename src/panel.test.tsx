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
import type { Changes } from "./panelData";
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
  branch: "feature",
  commit: null,
  base: "origin/main",
  added: 736,
  removed: 2,
  gh: true,
};

function withPanel(panel: Partial<PanelPrefs>): Profiles {
  return {
    ...SAVED,
    panel: { open: false, width: null, collapsed: [], ...panel },
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
      ...record("panel_pr", () => ({ number: 21, title: "Add the export" })),
      ...record("panel_connectors", (args) => ({
        servers:
          field(args, "account") === 1
            ? [{ name: "browser", kind: "stdio", scope: "user" }]
            : [],
        unreadable: [],
      })),
      ...record("panel_skills", () => [
        { name: "verify", description: "Run the gate", scope: "project" },
      ]),
      ...record("panel_open_pr", () => null),
      ...record("panel_open_folder", () => null),
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
      "panel_pr",
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
  const core = panelCore(withPanel({ open: true }));
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
    withPanel({ open: true, width: 360, collapsed: ["connectors"] }),
  );
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");

  expect(panel(root)?.style.getPropertyValue("--panel-width")).toBe("360px");
  expect(card(root, "Connectors").querySelector(".card-body")).toBeNull();
  expect(card(root, "Skills").querySelector(".card-body")).not.toBeNull();
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

  card(root, "Changes")
    .querySelector<HTMLButtonElement>("button.card-row")
    ?.click();
  await settle();

  const opened = core.calls.filter((c) => c.cmd === "panel_open_pr");
  expect(opened.map((c) => field(c.args, "folder"))).toEqual([ALPHA]);
});
