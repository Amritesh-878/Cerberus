import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import {
  cleanup,
  click,
  EMPTY,
  field,
  press,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: String.raw`C:\code\alpha` }],
  tabs: [
    {
      id: 2,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "powershell",
      cwd: String.raw`C:\code\alpha`,
      account: 1,
    },
  ],
  nextId: 3,
};

/** Setup part way, with a saved project, so there is somewhere a stray click could go. */
function midSetup(step: Profiles["setup"] = "accounts") {
  const steps: unknown[] = [];
  const opens: unknown[] = [];
  return {
    steps,
    opens,
    core: {
      tree_load: () => ({ tree: TREE, notice: null }),
      set_setup_step: (args: unknown) => {
        steps.push(field(args, "step"));
        return { ...SAVED, setup: field(args, "step") as Profiles["setup"] };
      },
      open_take: () => {
        opens.push(true);
        return null;
      },
    },
    profiles: () => ({ ...SAVED, setup: step }),
  };
}

function heading(root: HTMLElement): string | undefined {
  return root.querySelector("main h1")?.textContent;
}

test("a restart part way through setup reopens it at the step it reached", async () => {
  const setup = midSetup("accounts");
  const root = await renderApp(setup.profiles, setup.core);

  expect(heading(root)).toBe("Set up Cerberus");
  expect(root.querySelector(".steps .current .step-name")?.textContent).toBe(
    "Accounts",
  );
});

test("while setup runs the sidebar shows no projects, search or way out", async () => {
  const setup = midSetup();
  const root = await renderApp(setup.profiles, setup.core);

  expect(root.querySelector(".tab-row")).toBeNull();
  expect(root.querySelector(".sidebar .search")).toBeNull();
  expect(root.querySelector<HTMLElement>(".sidebar-foot")?.hidden).toBe(true);
  expect(root.querySelector(".tree")?.textContent).toContain(
    "once setup is done",
  );
});

test("Escape and the app's shortcuts do not leave setup", async () => {
  const setup = midSetup();
  const root = await renderApp(setup.profiles, setup.core);

  press("Escape");
  press("T", { ctrlKey: true, shiftKey: true });
  press("Tab", { ctrlKey: true });
  press("A", { ctrlKey: true, shiftKey: true });
  await settle();

  expect(heading(root)).toBe("Set up Cerberus");
  expect(root.querySelector(".new-tab")).toBeNull();
});

test("a cerberus open request waits until setup ends", async () => {
  const setup = midSetup();
  const root = await renderApp(setup.profiles, setup.core);
  expect(setup.opens).toEqual([]);

  click(root, "Skip setup");
  await settle();

  expect(setup.opens).toHaveLength(1);
});

test("each step reached is saved, and Skip ends setup in the core before leaving", async () => {
  const setup = midSetup("install");
  const root = await renderApp(setup.profiles, setup.core);

  click(root, "Next");
  await settle();
  click(root, "Skip setup");
  await settle();

  expect(setup.steps).toEqual(["accounts", null]);
  expect(heading(root)).toBe("Cerberus");
});

test("a skipped setup with no account says so on the start page and reopens there", async () => {
  const setup = midSetup("install");
  const root = await renderApp(() => ({ ...EMPTY, setup: "install" }), {
    ...setup.core,
    set_setup_step: (args: unknown) => ({
      ...EMPTY,
      setup: field(args, "step") as Profiles["setup"],
    }),
  });

  click(root, "Skip setup");
  await settle();
  expect(root.querySelector(".home-setup")?.textContent).toContain(
    "No account yet.",
  );
  click(root, "Finish setup");
  await settle();

  expect(root.querySelector(".steps .current .step-name")?.textContent).toBe(
    "Accounts",
  );
});

test("a failed save keeps setup open and says why", async () => {
  const setup = midSetup("install");
  const root = await renderApp(setup.profiles, {
    ...setup.core,
    set_setup_step: () => {
      throw new Error("disk full");
    },
  });

  click(root, "Skip setup");
  await settle();

  expect(heading(root)).toBe("Set up Cerberus");
  expect(root.textContent).toContain(
    "Setup progress not saved: Error: disk full",
  );
});

function sheetOpen(root: HTMLElement): boolean {
  return root.querySelector(".sheet") !== null;
}

function sheetChord(): void {
  press("?", { ctrlKey: true, shiftKey: true, code: "Slash" });
}

test("the shortcuts chord opens the sheet, and Escape or the chord again closes it", async () => {
  const root = await renderApp(() => SAVED);

  sheetChord();
  await settle();
  expect(root.querySelector(".sheet h2")?.textContent).toBe(
    "Keyboard shortcuts",
  );
  press("Escape");
  await settle();
  expect(sheetOpen(root)).toBe(false);
  sheetChord();
  await settle();
  sheetChord();
  await settle();

  expect(sheetOpen(root)).toBe(false);
});

test("the shortcuts sheet opens during setup too, without leaving it", async () => {
  const setup = midSetup();
  const root = await renderApp(setup.profiles, setup.core);

  sheetChord();
  await settle();

  expect(sheetOpen(root)).toBe(true);
  expect(heading(root)).toBe("Set up Cerberus");
});
