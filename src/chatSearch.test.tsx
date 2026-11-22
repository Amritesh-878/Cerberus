import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";

import { type FoundChat, merged, projectFor, whenText } from "./chatSearch";
import {
  activeTitle,
  cleanup,
  click,
  field,
  press,
  ptyCore,
  renderApp,
  SAVED,
  sendOn,
  settle,
  sidebarTitles,
} from "./harness";
import type { SavedTree } from "./saved";
import { matchTabs } from "./tabSearch";
import { emptyTree } from "./tree";

afterEach(cleanup);

const ALPHA = String.raw`C:\code\alpha`;

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

const OPEN: FoundChat = {
  id: "11111111-1111-4111-8111-111111111111",
  account: 1,
  title: "The planner chat",
  folder: ALPHA,
  folderExists: true,
  last: Date.parse("2026-11-08T10:00:00Z"),
  group: "text",
  score: 0,
  place: "text",
  marks: [],
  snippet: null,
  openIn: 3,
};

const ELSEWHERE: FoundChat = {
  id: "22222222-2222-4222-8222-222222222222",
  account: 0,
  title: "Fix the api tests",
  folder: String.raw`C:\code\alpha\api`,
  folderExists: true,
  last: Date.parse("2026-11-09T10:00:00Z"),
  group: "text",
  score: 0,
  place: "text",
  marks: [],
  snippet: "…the api tests now pass…",
  openIn: null,
};

const GONE: FoundChat = {
  ...ELSEWHERE,
  id: "33333333-3333-4333-8333-333333333333",
  title: "Old prototype",
  folder: String.raw`C:\old\proto`,
  folderExists: false,
  last: Date.parse("2026-10-01T10:00:00Z"),
  snippet: null,
};

function findCore(resume: (args: unknown) => unknown = () => null) {
  const pty = ptyCore();
  const calls: { cmd: string; args: unknown }[] = [];
  const finds: unknown[] = [];
  const record =
    (cmd: string, answer: (args: unknown) => unknown = () => null) =>
    (args: unknown) => {
      calls.push({ cmd, args });
      return answer(args);
    };
  return {
    pty,
    calls,
    finds,
    of: (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args),
    order: () => calls.map((c) => c.cmd),
    core: {
      ...pty.core,
      tree_load: () => ({ tree: TREE, notice: null }),
      tree_save: record("tree_save"),
      chat_find: (args: unknown) => {
        finds.push(args);
        return finds.length;
      },
      chat_find_close: record("chat_find_close"),
      chat_resume: record("chat_resume", resume),
      tab_move_folder: record("tab_move_folder"),
      "plugin:dialog|open": () => String.raw`C:\code\new-home`,
    },
  };
}

/** Plays the core's answer to the `nth` search: its chats, then done. */
function answer(
  find: ReturnType<typeof findCore>,
  nth: number,
  chats: FoundChat[],
): void {
  sendOn(
    find.finds[nth],
    "results",
    { kind: "found", search: nth + 1, chats },
    { kind: "done", search: nth + 1, searched: chats.length },
  );
}

function rows(root: HTMLElement): string[] {
  return [...root.querySelectorAll(".find-row .find-title")].map(
    (t) => t.textContent,
  );
}

async function openDialog(find: ReturnType<typeof findCore>) {
  const root = await renderApp(() => SAVED, find.core);
  press("F", { ctrlKey: true, shiftKey: true });
  await settle();
  answer(find, 0, [OPEN, ELSEWHERE, GONE]);
  await settle();
  return root;
}

test("Ctrl+Shift+F lists the open tabs, then every other chat newest first, and arrows move the choice", async () => {
  const find = findCore();
  const root = await openDialog(find);

  expect(field(find.finds[0], "query")).toBe("");
  // The planner's own chat shows once, as the planner tab.
  expect(rows(root)).toEqual(["planner", "Fix the api tests", "Old prototype"]);
  expect(
    [...root.querySelectorAll(".find-group")].map((g) => g.textContent),
  ).toEqual(["Open tabs", "Chats"]);
  expect(root.querySelector(".find-status")?.textContent).toBe("2 of 3 chats");
  press("ArrowDown");
  press("ArrowDown");
  press("ArrowDown");
  await settle();
  expect(
    root.querySelector('.find-row[aria-selected="true"] .find-title')
      ?.textContent,
  ).toBe("Old prototype");
  press("Escape");
  await settle();
  expect(root.querySelector(".find-chat")).toBeNull();
  expect(find.of("chat_find_close")).toHaveLength(1);
});

test("Enter on an open tab, the first row, switches to it", async () => {
  const find = findCore();
  const root = await openDialog(find);

  press("Enter");
  await settle();

  expect(activeTitle(root)).toBe("planner");
  expect(find.of("chat_resume")).toEqual([]);
  expect(root.querySelector(".find-chat")).toBeNull();
});

test("a chat opens in a new tab on its account, in the project holding its folder, saved before it resumes", async () => {
  const find = findCore();
  const root = await openDialog(find);

  press("ArrowDown");
  press("Enter");
  await settle();
  await settle();

  expect(sidebarTitles(root)).toEqual(["planner", "Claude Code · personal"]);
  // A tab's name is saved to disk, so the chat's title never becomes one.
  expect(JSON.stringify(find.of("tree_save"))).not.toContain(
    "Fix the api tests",
  );
  expect(find.of("chat_resume")).toEqual([{ tab: 4, session: ELSEWHERE.id }]);
  const order = find.order();
  expect(order.lastIndexOf("tree_save")).toBeLessThan(
    order.indexOf("chat_resume"),
  );
  const spawn = find.pty.spawned.at(-1);
  expect(field(spawn, "account")).toBe(0);
  expect(field(field(spawn, "request"), "cwd")).toBe(ELSEWHERE.folder);
  expect(activeTitle(root)).toBe("Claude Code · personal");
});

test("claude --resume for another account's chat opens it in a new tab there, as Find chat does", async () => {
  const find = findCore();
  const root = await renderApp(() => SAVED, find.core);

  await emitEvent("chat-elsewhere", ELSEWHERE);
  await settle();
  await settle();

  expect(find.of("chat_resume")).toEqual([{ tab: 4, session: ELSEWHERE.id }]);
  expect(field(find.pty.spawned.at(-1), "account")).toBe(0);
  expect(activeTitle(root)).toBe("Claude Code · personal");
});

test("a chat whose folder is gone asks for one, then moves the chat there before it resumes", async () => {
  const find = findCore();
  const root = await openDialog(find);
  press("ArrowDown");
  press("ArrowDown");

  press("Enter");
  await settle();
  expect(root.querySelector(".find-gone")?.textContent).toContain(
    "Its folder, proto, is gone.",
  );
  expect(find.of("chat_resume")).toEqual([]);
  click(root.querySelector(".find-gone") ?? root, "Choose a folder…");
  await settle();
  await settle();

  // The new folder makes a new project, id 4, so the tab is 5.
  expect(find.of("chat_resume")).toEqual([{ tab: 5, session: GONE.id }]);
  expect(find.of("tab_move_folder")).toEqual([
    { tab: 5, folder: String.raw`C:\code\new-home` },
  ]);
  const spawn = find.pty.spawned.at(-1);
  expect(field(field(spawn, "request"), "cwd")).toBe(
    String.raw`C:\code\new-home`,
  );
});

test("a chat the core won't resume leaves no tab behind and says why", async () => {
  const find = findCore(() => {
    throw new Error("that chat is not in this tab's folder on its account");
  });
  const root = await openDialog(find);

  press("ArrowDown");
  press("Enter");
  await settle();
  await settle();

  expect(sidebarTitles(root)).toEqual(["planner"]);
  expect(root.querySelector(".find-chat .error")?.textContent).toContain(
    "not in this tab's folder",
  );
});

test("a refused open of a chat from a new folder takes its new project away too", async () => {
  const find = findCore(() => {
    throw new Error("that chat is not in this tab's folder on its account");
  });
  const root = await renderApp(() => SAVED, find.core);
  press("F", { ctrlKey: true, shiftKey: true });
  await settle();
  answer(find, 0, [
    {
      ...ELSEWHERE,
      folder: String.raw`D:\other\repo`,
    },
  ]);
  await settle();

  press("ArrowDown");
  press("Enter");
  await settle();
  await settle();

  expect(root.querySelector(".find-chat .error")).not.toBeNull();
  expect(sidebarTitles(root)).toEqual(["planner"]);
  expect(JSON.stringify(find.of("tree_save").at(-1))).not.toContain("repo");
});

test("a refused open keeps its new project when another chat opened into it meanwhile", async () => {
  let refuse: (error: Error) => void = () => undefined;
  let resumes = 0;
  const find = findCore(() => {
    resumes += 1;
    return resumes === 1
      ? new Promise((_, reject) => {
          refuse = reject;
        })
      : null;
  });
  const root = await renderApp(() => SAVED, find.core);
  press("F", { ctrlKey: true, shiftKey: true });
  await settle();
  answer(find, 0, [{ ...ELSEWHERE, folder: String.raw`D:\other\repo` }]);
  await settle();
  press("ArrowDown");
  press("Enter");
  await settle();
  press("Escape");
  await settle();
  press("F", { ctrlKey: true, shiftKey: true });
  await settle();
  answer(find, 1, [
    { ...GONE, folder: String.raw`D:\other\repo\api`, folderExists: true },
  ]);
  await settle();
  // The first open's tab is listed now too, so the chat is third.
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  await settle();
  await settle();

  refuse(new Error("that chat is not in this tab's folder on its account"));
  await settle();
  await settle();

  expect(resumes).toBe(2);
  expect(sidebarTitles(root)).toHaveLength(2);
  expect(JSON.stringify(find.of("tree_save").at(-1))).toContain("repo");
});

test("Enter on a focused button or while composing opens no chat", async () => {
  const find = findCore();
  const root = await openDialog(find);
  const close = root.querySelector<HTMLButtonElement>(
    '.find-head [aria-label="Close"]',
  );
  if (!close) throw new Error("no Close button");

  close.focus();
  close.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
  );
  root.querySelector(".find-chat input")?.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      isComposing: true,
    }),
  );
  await settle();

  expect(find.of("chat_resume")).toEqual([]);
  expect(sidebarTitles(root)).toEqual(["planner"]);
});

test("typing searches again, and an older search's results are dropped", async () => {
  const find = findCore();
  const root = await openDialog(find);
  const box = root.querySelector<HTMLInputElement>(".find-chat input");
  if (!box) throw new Error("no search box");

  box.value = "api tests";
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 200));
  await settle();
  answer(find, 1, [ELSEWHERE]);
  answer(find, 0, [OPEN]);
  await settle();

  expect(field(find.finds[1], "query")).toBe("api tests");
  expect(rows(root)).toEqual(["Fix the api tests"]);
  expect(root.querySelector(".find-snippet")?.textContent).toBe(
    "…the api tests now pass…",
  );
});

/** Types into the box and waits out the typing pause. */
async function typeQuery(root: HTMLElement, text: string): Promise<void> {
  const box = root.querySelector<HTMLInputElement>(".find-chat input");
  if (!box) throw new Error("no search box");
  box.value = text;
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 200));
  await settle();
}

test("a row says where it matched and marks the typed letters there", async () => {
  const find = findCore();
  const root = await openDialog(find);

  await typeQuery(root, "fxapi");
  answer(find, 1, [
    {
      ...ELSEWHERE,
      group: "name",
      score: 90,
      place: "title",
      marks: [
        [0, 1],
        [2, 1],
        [8, 3],
      ],
    },
    {
      ...GONE,
      place: "text",
      snippet: "an api in the old proto",
      marks: [[3, 3]],
    },
  ]);
  await settle();

  const marked = [...root.querySelectorAll(".find-row mark")].map(
    (m) => m.textContent,
  );
  expect(marked).toEqual(["F", "x", "api", "api"]);
  expect(root.querySelector(".find-where")?.textContent).toBe("title");
});

test("close matches show under their own heading, only when nothing had every word", async () => {
  const find = findCore();
  const root = await openDialog(find);

  await typeQuery(root, "api giraffe");
  answer(find, 1, [{ ...ELSEWHERE, group: "close", score: 101 }]);
  await settle();

  const groups = [...root.querySelectorAll(".find-group")].map(
    (g) => g.textContent,
  );
  expect(groups).toContain("Close matches");
  expect(groups).not.toContain("Chats");
  expect(root.querySelector(".find-status")?.textContent).toBe(
    "No chat has every word",
  );
});

test("one letter filters the open tabs and leaves chats unsearched", async () => {
  const find = findCore();
  const root = await openDialog(find);

  await typeQuery(root, "p");

  expect(find.finds).toHaveLength(1);
  expect(rows(root)).toEqual(["planner"]);
  expect(root.querySelector(".find-status")?.textContent).toBe(
    "Type two letters to search chats",
  );
});

test("the helpers: merging, the holding project and the day", () => {
  expect(merged([OPEN], [ELSEWHERE, OPEN]).map((c) => c.id)).toEqual([
    ELSEWHERE.id,
    OPEN.id,
  ]);
  const ranked = (
    chat: FoundChat,
    group: FoundChat["group"],
    score: number,
  ): FoundChat => ({ ...chat, group, score });
  // Name matches, then text, then close ones; within a group the better score, then the newer chat.
  expect(
    merged(
      [ranked(ELSEWHERE, "text", 1000), ranked(GONE, "name", 40)],
      [ranked(OPEN, "text", 1000), ranked(GONE, "name", 40)],
    ).map((c) => c.id),
  ).toEqual([GONE.id, ELSEWHERE.id, OPEN.id]);
  expect(
    merged([ranked(OPEN, "close", 200)], [ranked(GONE, "text", 0)]).map(
      (c) => c.id,
    ),
  ).toEqual([GONE.id, OPEN.id]);
  const tree = {
    ...emptyTree(),
    projects: [
      { id: 1, name: "code", folder: String.raw`C:\code` },
      { id: 2, name: "alpha", folder: "C:\\Code\\Alpha\\" },
    ],
    nextId: 3,
  };
  expect(projectFor(tree, String.raw`c:\code\alpha\api`)[1]).toBe(2);
  expect(projectFor(tree, "C:/code/beta")[1]).toBe(1);
  const [added, id] = projectFor(tree, String.raw`D:\elsewhere`);
  expect(id).toBe(3);
  expect(added.projects.at(-1)?.name).toBe("elsewhere");
  const now = Date.parse("2026-11-09T15:00:00");
  expect(whenText(now - 86_400_000, now)).toBe("yesterday");
  expect(whenText(Date.parse("2025-03-02T10:00:00"), now, "en-GB")).toBe(
    "2 Mar 2025",
  );
});

test("open tabs match each word's letters in order in a title, project, folder name or account, best first", () => {
  const tab = {
    id: 3,
    title: "planner",
    project: "alpha",
    folder: String.raw`C:\code\alpha`,
    account: "work",
    accountId: 1,
    program: "claude",
  };
  const shell = {
    ...tab,
    id: 4,
    title: "shell",
    account: null,
    program: "powershell",
  };
  const notes = { ...tab, id: 5, title: "the planner notes" };
  const titles = (query: string) =>
    matchTabs([tab, shell, notes], query).map((hit) => hit.tab.title);

  expect(titles("")).toEqual(["planner", "shell", "the planner notes"]);
  expect(titles("PLAN")).toEqual(["planner", "the planner notes"]);
  expect(titles("pnr")).toEqual(["planner", "the planner notes"]);
  expect(titles("sh")).toEqual(["shell"]);
  expect(titles("alpha work")).toEqual(["planner", "the planner notes"]);
  expect(titles("code"), "a folder's name, not its whole path").toEqual([]);
  expect(titles("planner personal")).toEqual([]);
  expect(matchTabs([tab], "pnr")[0]?.marks).toEqual([
    [0, 1],
    [3, 1],
    [6, 1],
  ]);
});
