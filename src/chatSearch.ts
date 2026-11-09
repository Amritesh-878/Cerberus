import { Channel, invoke } from "@tauri-apps/api/core";

import type { AccountId } from "./accounts";
import { addProject, type ProjectId, type TabId, type Tree } from "./tree";

/** One chat the core found; its text came from the transcript and stays in the dialog. */
export interface FoundChat {
  id: string;
  account: AccountId | null;
  title: string;
  folder: string | null;
  folderExists: boolean;
  /** Milliseconds since the epoch of its last message. */
  last: number;
  /** How many typed words it matched, exactly or by one edit. */
  hits: number;
  /** How many of those matched exactly. */
  exact: number;
  snippet: string | null;
  /** A tab that already has this chat. */
  openIn: TabId | null;
}

export type FindBatch =
  | { kind: "found"; search: number; chats: FoundChat[] }
  | { kind: "done"; search: number; searched: number };

/** Starts a search; batches of this search and any later one arrive on `onBatch`. */
export function startFind(
  query: string,
  onBatch: (batch: FindBatch) => void,
): Promise<number> {
  const results = new Channel<FindBatch>();
  results.onmessage = onBatch;
  return invoke<number>("chat_find", { query, results });
}

/** The dialog closed, so the core stops searching and forgets what it read. */
export function closeFind(): Promise<void> {
  return invoke("chat_find_close");
}

/** Gives a saved, not yet started tab the found chat to resume. */
export function resumeFound(tab: TabId, session: string): Promise<void> {
  return invoke("chat_resume", { tab, session });
}

/** Adds a batch to the list, one row per chat and account: more words, more exact, then newest. */
export function merged(
  list: readonly FoundChat[],
  more: readonly FoundChat[],
): FoundChat[] {
  const key = (c: FoundChat) => `${String(c.account)}/${c.id}`;
  const seen = new Set(more.map(key));
  return [...list.filter((c) => !seen.has(key(c))), ...more].sort(
    (a, b) => b.hits - a.hits || b.exact - a.exact || b.last - a.last,
  );
}

// Windows paths compare without case or slash direction, as Explorer does.
function normal(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

/** The project whose folder holds `folder`, the deepest one if several do, else a new one for it. */
export function projectFor(tree: Tree, folder: string): [Tree, ProjectId] {
  const wanted = normal(folder);
  const holding = tree.projects
    .filter((p) => {
      const own = normal(p.folder);
      return wanted === own || wanted.startsWith(`${own}/`);
    })
    .sort((a, b) => b.folder.length - a.folder.length)[0];
  return holding ? [tree, holding.id] : addProject(tree, folder);
}

/** "today 14:05", "yesterday", or a date, for a result's last message. */
export function whenText(
  last: number,
  now = Date.now(),
  locale?: string,
): string {
  const day = (t: number) => new Date(t).toDateString();
  if (day(last) === day(now)) {
    return new Date(last).toLocaleTimeString(locale, {
      hour: "numeric",
      minute: "2-digit",
    });
  }
  if (day(last) === day(now - 86_400_000)) return "yesterday";
  return new Date(last).toLocaleDateString(locale, {
    day: "numeric",
    month: "short",
    year:
      new Date(last).getFullYear() === new Date(now).getFullYear()
        ? undefined
        : "numeric",
  });
}
