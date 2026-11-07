import { afterEach, expect, test } from "vitest";

import {
  activeTitle,
  cleanup,
  click,
  field,
  open,
  ptyCore,
  renderApp,
  row,
  SAVED,
  settle,
} from "./harness";
import type { SavedTree } from "./saved";

afterEach(cleanup);

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: String.raw`C:\code\p1` }],
  tabs: [
    {
      id: 3,
      projectId: 1,
      parentId: null,
      title: "planner",
      program: "claude",
      cwd: null,
      account: 0,
    },
    {
      id: 6,
      projectId: 1,
      parentId: null,
      title: "shell",
      program: "powershell",
      cwd: null,
      account: 1,
    },
  ],
  nextId: 7,
};

function core(extra: Record<string, (args: unknown) => unknown> = {}) {
  const pty = ptyCore();
  return {
    pty,
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      ...extra,
    },
  };
}

const views = (root: HTMLElement) =>
  [...root.querySelectorAll<HTMLElement>(".tab-view")].map((v) =>
    [...v.classList].filter((c) => c !== "tab-view").join(" "),
  );

/** Lays the chat area out at x 200..1000, y 40..640, as happy-dom has no layout. */
function layOut(root: HTMLElement): void {
  const stage = root.querySelector<HTMLElement>(".stage");
  if (!stage) throw new Error("no stage");
  stage.getBoundingClientRect = () => new DOMRect(200, 40, 800, 600);
}

/** Drags the tab's row by pointer from the sidebar to (x, y), checking the drop hint on the way. */
async function dragTo(
  root: HTMLElement,
  title: string,
  x: number,
  y: number,
): Promise<string | undefined> {
  const handle = row(root, title).querySelector<HTMLElement>(".tab-select");
  if (!handle) throw new Error("no handle");
  handle.setPointerCapture = () => undefined;
  handle.hasPointerCapture = () => false;
  const at = (type: string, clientX: number, clientY: number) => {
    handle.dispatchEvent(
      new PointerEvent(type, { bubbles: true, clientX, clientY, button: 0 }),
    );
  };
  at("pointerdown", 40, 100);
  at("pointermove", 120, 110);
  at("pointermove", x, y);
  await settle();
  const hint = root.querySelector(".split-drop")?.className;
  at("pointerup", x, y);
  handle.click();
  await settle();
  return hint;
}

test("a tab dragged onto the right half of the chat opens beside the shown tab", async () => {
  const app = core();
  const root = await renderApp(() => SAVED, app.core);
  await open(root, "planner");
  layOut(root);

  expect(await dragTo(root, "shell", 800, 300)).toBe("split-drop over");

  // The click that ends the drag doesn't select the dragged tab.
  expect(activeTitle(root)).toBe("planner");
  expect(views(root)).toEqual(["shown split-left", "shown split-right"]);
  expect(root.querySelector(".split-drop")).toBeNull();
  // The dragged tab's program started, so its half is not blank.
  expect(app.pty.spawned.map((s) => field(s, "tab"))).toEqual([3, 6]);
});

test("a tab dropped on the left half or outside the chat opens no split", async () => {
  const root = await renderApp(() => SAVED, core().core);
  await open(root, "planner");
  layOut(root);

  expect(await dragTo(root, "shell", 400, 300)).toBe("split-drop");
  expect(await dragTo(root, "shell", 100, 300)).toBe("split-drop");

  expect(root.querySelector(".split-bar")).toBeNull();
  expect(activeTitle(root)).toBe("planner");
});

test("the shown tab dragged onto itself does nothing", async () => {
  const root = await renderApp(() => SAVED, core().core);
  await open(root, "planner");
  layOut(root);

  expect(await dragTo(root, "planner", 800, 300)).toBeUndefined();
  expect(root.querySelector(".split-bar")).toBeNull();
});

test("the divider moves, resets to half on a double-click, and is saved", async () => {
  const shares: unknown[] = [];
  const app = core({
    set_split_share: (args: unknown) => {
      shares.push(field(args, "share"));
      return { ...SAVED, splitShare: field(args, "share") };
    },
  });
  const root = await renderApp(() => ({ ...SAVED, splitShare: 620 }), app.core);
  await open(root, "planner");
  layOut(root);
  await dragTo(root, "shell", 800, 300);
  const stage = root.querySelector<HTMLElement>(".stage");
  const edge = root.querySelector<HTMLElement>(".split-edge");
  expect(stage?.style.getPropertyValue("--split-share")).toBe("62%");

  edge?.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }),
  );
  await settle();
  expect(stage?.style.getPropertyValue("--split-share")).toBe("60%");

  if (!edge) throw new Error("no edge");
  edge.setPointerCapture = () => undefined;
  edge.hasPointerCapture = () => false;
  edge.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  edge.dispatchEvent(
    new PointerEvent("pointermove", { bubbles: true, clientX: 520 }),
  );
  edge.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  await settle();
  expect(stage?.style.getPropertyValue("--split-share")).toBe("40%");

  edge.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await settle();

  expect(shares).toEqual([600, 400, 500]);
  expect(stage?.style.getPropertyValue("--split-share")).toBe("50%");
});

test("the divider shows only while a split does", async () => {
  const root = await renderApp(() => SAVED, core().core);
  await open(root, "planner");
  layOut(root);
  expect(root.querySelector(".split-edge")).toBeNull();

  await dragTo(root, "shell", 800, 300);
  expect(root.querySelector(".split-edge")).not.toBeNull();

  click(root, "Close split");
  await settle();
  expect(root.querySelector(".split-edge")).toBeNull();
});
