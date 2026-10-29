import type { TabId } from "./tree";

export type LiveState =
  | { state: "working" }
  | { state: "idle" }
  | { state: "limited"; resetsAt: number | null };

type Life = LiveState | { state: "done" };

interface Entry {
  life: Life;
  /** The chat finished a turn while its tab was not shown, and the tab has not been opened since. */
  waiting: boolean;
}

export type Board = ReadonlyMap<TabId, Entry>;

export type Mark =
  | { kind: "working" }
  | { kind: "waiting" }
  | { kind: "limited"; resetsAt: number | null }
  | { kind: "done" };

export function reported(
  board: Board,
  tab: TabId,
  live: LiveState,
  shown: TabId | undefined,
): Board {
  const before = board.get(tab);
  const finished = before?.life.state === "working" && live.state !== "working";
  const waiting =
    live.state !== "working" &&
    tab !== shown &&
    (finished || (before?.waiting ?? false));
  return new Map(board).set(tab, { life: live, waiting });
}

export function exited(board: Board, tab: TabId): Board {
  return new Map(board).set(tab, { life: { state: "done" }, waiting: false });
}

export function opened(board: Board, tab: TabId): Board {
  const entry = board.get(tab);
  if (!entry?.waiting) return board;
  return new Map(board).set(tab, { ...entry, waiting: false });
}

/** Working shows over everything; a limit says more than waiting; idle shows nothing. */
export function markOf(board: Board, tab: TabId): Mark | undefined {
  const entry = board.get(tab);
  if (!entry) return undefined;
  const { life } = entry;
  if (life.state === "working" || life.state === "done") {
    return { kind: life.state };
  }
  if (life.state === "limited") {
    return { kind: "limited", resetsAt: life.resetsAt };
  }
  return entry.waiting ? { kind: "waiting" } : undefined;
}

/** The next tab after `from`, in sidebar order and wrapping, whose chat is waiting on you. */
export function nextWaiting(
  board: Board,
  order: readonly TabId[],
  from: TabId | undefined,
): TabId | undefined {
  const at = from === undefined ? -1 : order.indexOf(from);
  const after = [...order.slice(at + 1), ...order.slice(0, at + 1)];
  return after.find(
    (id) => id !== from && markOf(board, id)?.kind === "waiting",
  );
}

/** Tabs whose program runs: each starts when first shown, so a restored tab never opened costs nothing. */
export function startedTabs(
  started: ReadonlySet<TabId>,
  shown: TabId | undefined,
  open: readonly TabId[],
): ReadonlySet<TabId> {
  const kept = [...started].filter((id) => open.includes(id));
  const adding = shown !== undefined && !started.has(shown);
  if (!adding && kept.length === started.size) return started;
  return new Set(adding ? [...kept, shown] : kept);
}

export function resetText(resetsAt: number, locale?: string): string {
  return new Date(resetsAt * 1000)
    .toLocaleTimeString(locale, { hour: "numeric", minute: "2-digit" })
    .replace(/\s/g, "")
    .toLowerCase();
}
