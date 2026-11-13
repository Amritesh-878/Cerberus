import { StateEffect } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { changeBetween, languageOf, refusal } from "./FilePane";
import type { PanelPrefs, Profiles } from "./accounts";
import {
  button,
  cleanup,
  field,
  open,
  ptyCore,
  renderApp,
  SAVED,
  settle,
} from "./harness";
import type { FileView, LiveView, Output } from "./live";
import type { SavedTree } from "./saved";

const startWidth = window.innerWidth;
function setWidth(width: number): void {
  Object.defineProperty(window, "innerWidth", {
    value: width,
    configurable: true,
  });
}
// Wide enough that the panel sits beside the chat rather than over it.
beforeEach(() => {
  setWidth(1600);
});
afterEach(async () => {
  await cleanup();
  setWidth(startWidth);
});

const ALPHA = String.raw`C:\code\alpha`;
const APP = String.raw`C:\code\alpha\src\app.ts`;
const README = String.raw`C:\code\alpha\readme.md`;

const TREE: SavedTree = {
  projects: [{ id: 1, name: "alpha", folder: ALPHA }],
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
  ],
  nextId: 4,
};

const OPEN_PANEL: Profiles = {
  ...SAVED,
  panel: { ...SAVED.panel, open: true },
};

const APP_OUT: Output = { name: "app.ts", folder: "src", path: APP, at: 1 };
const README_OUT: Output = {
  name: "readme.md",
  folder: "",
  path: README,
  at: 2,
};

function liveView(outputs: Output[]): LiveView {
  return {
    tabs: [
      {
        tab: 3,
        account: 1,
        context: null,
        model: null,
        keptTheirs: false,
        outputs,
        tasks: [],
        done: [],
        finished: 0,
      },
    ],
    meters: [],
    warnings: [],
  };
}

/** A core whose files are fake text the test sets, with stamps the test moves. */
function fileCore(files: Record<string, FileView>) {
  const pty = ptyCore();
  const reads: unknown[] = [];
  const opened: unknown[] = [];
  const stamps: Record<string, string> = {};
  let profiles = OPEN_PANEL;
  return {
    files,
    stamps,
    reads,
    opened,
    profiles: () => profiles,
    core: {
      ...pty.core,
      set_panel: (args: unknown) => {
        const panel = field(args, "panel") as PanelPrefs;
        profiles = { ...profiles, panel, revision: profiles.revision + 1 };
        return profiles;
      },
      tree_load: () => ({ tree: TREE, notice: null }),
      panel_changes: () => ({ kind: "notRepo" }),
      panel_prs: () => [],
      panel_connectors: () => ({ servers: [], unreadable: [] }),
      panel_skills: () => [],
      live_read_output: (args: unknown) => {
        reads.push(args);
        return files[String(field(args, "path"))] ?? { kind: "gone" };
      },
      live_output_stamp: (args: unknown) =>
        stamps[String(field(args, "path"))] ?? null,
      live_open_output: (args: unknown) => {
        opened.push(args);
        return null;
      },
    },
  };
}

async function start(files: Record<string, FileView>, outputs: Output[]) {
  const core = fileCore(files);
  const root = await renderApp(core.profiles, core.core);
  await open(root, "planner");
  await emitEvent("live", liveView(outputs));
  await settle();
  return { root, ...core };
}

function outputRow(root: HTMLElement, name: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".output-row")].find(
    (r) => r.querySelector(".row-main")?.textContent === name,
  );
  if (!found) throw new Error(`no ${name} row`);
  return found;
}

/** Clicks a written file and waits for the lazy pane to load and show it. */
async function openFile(root: HTMLElement, name: string): Promise<HTMLElement> {
  outputRow(root, name)
    .querySelector<HTMLButtonElement>(".output-open")
    ?.click();
  return vi.waitFor(() => {
    const pane = root.querySelector<HTMLElement>(".file-view");
    if (!pane?.querySelector(".file-name")) throw new Error("no pane yet");
    return pane;
  });
}

function code(pane: HTMLElement): string {
  return [...pane.querySelectorAll(".cm-line")]
    .map((l) => l.textContent)
    .join("\n");
}

const text = (body: string, base: string | null = null): FileView => ({
  kind: "text",
  text: body,
  base,
});

test("a click in Outputs opens the file beside the chat, read only, by its Outputs entry", async () => {
  const app = await start({ [APP]: text("export const x = 2;\n") }, [APP_OUT]);

  const pane = await openFile(app.root, "app.ts");
  await vi.waitFor(() => {
    expect(code(pane)).toContain("export const x = 2;");
  });

  expect(app.reads).toEqual([{ tab: 3, path: APP }]);
  expect(pane.classList.contains("split-right")).toBe(true);
  expect(app.root.querySelector(".tab-view.split-left")).not.toBeNull();
  expect(pane.querySelector(".file-name")?.textContent).toBe("app.ts");
  expect(pane.querySelector(".cm-content")?.getAttribute("aria-readonly")).toBe(
    "true",
  );
  await vi.waitFor(() => {
    expect(pane.querySelector(".cm-line span[class]")?.textContent).toBe(
      "export",
    );
  });
  expect(outputRow(app.root, "app.ts").classList.contains("opened")).toBe(true);
  expect(app.opened).toEqual([]);
});

test("Close and Escape in the pane shut it; Escape elsewhere leaves it", async () => {
  const app = await start({ [APP]: text("a\n") }, [APP_OUT]);

  let pane = await openFile(app.root, "app.ts");
  button(pane, "Close").click();
  await settle();
  expect(app.root.querySelector(".file-view")).toBeNull();
  expect(app.root.querySelector(".tab-view.split-left")).toBeNull();

  pane = await openFile(app.root, "app.ts");
  document.body.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
  );
  await settle();
  expect(app.root.querySelector(".file-view")).not.toBeNull();

  pane
    .querySelector(".file-pane")
    ?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
  await settle();
  expect(app.root.querySelector(".file-view")).toBeNull();
});

test("Diff shows the change against HEAD inline, and an untracked file as all new", async () => {
  const app = await start(
    {
      [APP]: text("one\ntwo changed\nthree\n", "one\ntwo\nthree\n"),
      [README]: text("# new\n"),
    },
    [README_OUT, APP_OUT],
  );

  const pane = await openFile(app.root, "app.ts");
  await vi.waitFor(() => {
    expect(code(pane)).toContain("two changed");
  });
  expect(pane.querySelector(".cm-changedLine")).toBeNull();

  button(pane, "Diff").click();
  await settle();
  expect(button(pane, "Diff").getAttribute("aria-pressed")).toBe("true");
  await vi.waitFor(() => {
    expect(pane.querySelector(".cm-changedLine")?.textContent).toBe(
      "two changed",
    );
    expect(pane.querySelector(".cm-deletedChunk")?.textContent).toContain(
      "two",
    );
  });

  outputRow(app.root, "readme.md")
    .querySelector<HTMLButtonElement>(".output-open")
    ?.click();
  await vi.waitFor(() => {
    expect(code(pane)).toBe("# new\n");
    expect(pane.querySelector(".file-note")?.textContent).toBe(
      "Not in git: all new",
    );
    expect(pane.querySelector(".cm-changedLine")?.textContent).toBe("# new");
  });

  button(pane, "Diff").click();
  await settle();
  expect(pane.querySelector(".cm-changedLine")).toBeNull();
  expect(pane.querySelector(".file-note")).toBeNull();
});

test("Follow switches to each new file the chat writes", async () => {
  const app = await start(
    { [APP]: text("app\n"), [README]: text("readme\n") },
    [APP_OUT],
  );
  const pane = await openFile(app.root, "app.ts");
  await vi.waitFor(() => {
    expect(code(pane)).toBe("app\n");
  });

  // Without Follow a new file leaves the pane where it is.
  await emitEvent("live", liveView([README_OUT, APP_OUT]));
  await settle();
  expect(pane.querySelector(".file-name")?.textContent).toBe("app.ts");

  button(pane, "Follow").click();
  await vi.waitFor(() => {
    expect(pane.querySelector(".file-name")?.textContent).toBe("readme.md");
    expect(code(pane)).toBe("readme\n");
  });

  await emitEvent("live", liveView([{ ...APP_OUT, at: 3 }, README_OUT]));
  await vi.waitFor(() => {
    expect(pane.querySelector(".file-name")?.textContent).toBe("app.ts");
  });
  expect(app.reads.map((r) => field(r, "path"))).toEqual([APP, README, APP]);
});

test("a change on disk reloads in place and keeps the scroll place", async () => {
  const before = Array.from({ length: 200 }, (_, i) => `line ${String(i)}`);
  const app = await start({ [APP]: text(before.join("\n")) }, [APP_OUT]);
  app.stamps[APP] = "1-100";
  const pane = await openFile(app.root, "app.ts");
  await vi.waitFor(() => {
    expect(code(pane)).toContain("line 0");
  });
  const scroller = pane.querySelector<HTMLElement>(".cm-scroller");
  const dom = pane.querySelector<HTMLElement>(".cm-editor");
  if (!scroller || !dom) throw new Error("no editor");
  scroller.scrollTop = 600;
  // happy-dom lays nothing out, so the test checks how the reload is made; the real webview checks the pixels.
  const editor = EditorView.findFromDOM(dom);
  if (!editor) throw new Error("no editor view");
  const place = editor.state.doc.line(180).from;
  editor.dispatch({ selection: { anchor: place } });
  const scrolls: boolean[] = [];
  editor.dispatch({
    effects: StateEffect.appendConfig.of(
      EditorView.updateListener.of((update) => {
        for (const tr of update.transactions)
          if (tr.docChanged) scrolls.push(tr.scrollIntoView);
      }),
    ),
  });
  const snapshot = vi.spyOn(editor, "scrollSnapshot");

  const after = [...before];
  after[150] = "line 150 changed";
  app.files[APP] = text(after.join("\n"));
  app.stamps[APP] = "2-116";

  await vi.waitFor(
    () => {
      expect(app.reads).toHaveLength(2);
    },
    { timeout: 3000 },
  );
  await settle();
  expect(editor.state.doc.line(151).text).toBe("line 150 changed");
  expect(scrolls).toEqual([false]);
  expect(snapshot).toHaveBeenCalledOnce();
  expect(editor.state.selection.main.anchor).toBe(
    editor.state.doc.line(180).from,
  );
  expect(editor.state.doc.line(180).from).toBe(place + 8);
  expect(scroller.scrollTop).toBe(600);

  // The chat's own write reloads without waiting for the poll.
  app.files[APP] = text("short\n");
  await emitEvent("live", liveView([{ ...APP_OUT, at: 5 }]));
  await vi.waitFor(() => {
    expect(app.reads).toHaveLength(3);
  });
});

test("over a narrow chat the panel closes so the pane it opened shows", async () => {
  setWidth(700);
  const app = await start({ [APP]: text("a\n") }, [APP_OUT]);
  expect(app.root.querySelector(".panel")).not.toBeNull();

  await openFile(app.root, "app.ts");
  await settle();

  expect(app.root.querySelector(".panel")).toBeNull();
  expect(app.profiles().panel.open).toBe(false);
});

test("a large or binary file says so and offers only VS Code", async () => {
  const app = await start(
    {
      [APP]: { kind: "tooBig", size: 3 * 1024 * 1024 },
      [README]: { kind: "binary" },
    },
    [README_OUT, APP_OUT],
  );

  const pane = await openFile(app.root, "app.ts");
  await vi.waitFor(() => {
    expect(pane.querySelector(".file-message")?.textContent).toContain(
      "This file is 3.0 MB, over the 2 MB the pane shows.",
    );
  });
  expect(pane.querySelector<HTMLElement>(".file-code")?.hidden).toBe(true);
  button(pane, "Open in VS Code").click();
  await settle();
  expect(app.opened).toEqual([{ tab: 3, path: APP }]);
});

test("the row's VS Code button opens the file there, not in the pane", async () => {
  const app = await start({}, [APP_OUT]);

  button(outputRow(app.root, "app.ts"), "Open app.ts in VS Code").click();
  await settle();

  expect(app.opened).toEqual([{ tab: 3, path: APP }]);
  expect(app.root.querySelector(".file-view")).toBeNull();
  expect(app.reads).toEqual([]);
});

test("the language by extension, the one changed span and the refusals", () => {
  expect(languageOf(APP)).toBe("ts");
  expect(languageOf("/x/Main.RS")).toBe("rs");
  expect(languageOf(String.raw`C:\a.b\notes`)).toBeUndefined();
  expect(languageOf("/x/.gitignore")).toBeUndefined();
  expect(languageOf("/x/data.csv")).toBeUndefined();

  expect(changeBetween("abc", "abc")).toBeUndefined();
  expect(changeBetween("a\nb\nc", "a\nB\nc")).toEqual({
    from: 2,
    to: 3,
    insert: "B",
  });
  expect(changeBetween("aaa", "aaaa")).toEqual({ from: 3, to: 3, insert: "a" });

  expect(refusal(text("x"))).toBeUndefined();
  expect(refusal({ kind: "binary" })).toMatch(/binary/);
  expect(refusal({ kind: "gone" })).toMatch(/gone/);
  expect(refusal({ kind: "private" })).toMatch(/never reads/);
});
