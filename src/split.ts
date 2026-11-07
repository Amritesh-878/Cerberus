import { createSignal } from "solid-js";

import type { TabId } from "./tree";

export type Pair = readonly [TabId, TabId];

/** The pair after a message between `from` and `to`; a reply between the same two keeps their sides. */
export function nextPair(
  current: Pair | undefined,
  from: TabId,
  to: TabId,
): Pair {
  if (current?.includes(from) && current.includes(to)) return current;
  return [from, to];
}

/** Whether a tab dropped at (x, y) splits with the shown one: anywhere on the right half of the chat area. */
export function dropsToSplit(
  x: number,
  y: number,
  area: { left: number; top: number; width: number; height: number },
): boolean {
  return (
    x >= area.left + area.width / 2 &&
    x <= area.left + area.width &&
    y >= area.top &&
    y <= area.top + area.height
  );
}

/** Two chats side by side whenever one of them is shown, kept while other tabs show, until closed. */
export function createSplit(shown: () => TabId | undefined) {
  const [pair, setPair] = createSignal<Pair>();
  const showing = (): Pair | undefined => {
    const now = pair();
    const id = shown();
    return now && id !== undefined && now.includes(id) ? now : undefined;
  };
  return {
    showing,
    has: (id: TabId) => showing()?.includes(id) ?? false,
    side: (id: TabId): "left" | "right" | undefined => {
      const now = showing();
      return now?.[0] === id ? "left" : now?.[1] === id ? "right" : undefined;
    },
    /** The half beside the shown tab, which must be running for the split to show it. */
    beside: (): TabId | undefined => {
      const now = showing();
      return now?.find((id) => id !== shown());
    },
    open: (from: TabId, to: TabId) => {
      if (from !== to) setPair((now) => nextPair(now, from, to));
    },
    close: () => {
      setPair(undefined);
    },
    /** A closed tab takes the split with it, so no half stays blank. */
    forget: (id: TabId) => {
      setPair((now) => (now?.includes(id) ? undefined : now));
    },
  };
}
