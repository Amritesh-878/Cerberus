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

export type Side = "left" | "right";

/** Which half of the chat area a tab dropped at (x, y) lands on, to split with the shown one there; outside it, none. */
export function dropSide(
  x: number,
  y: number,
  area: { left: number; top: number; width: number; height: number },
): Side | undefined {
  const inside =
    x >= area.left &&
    x <= area.left + area.width &&
    y >= area.top &&
    y <= area.top + area.height;
  if (!inside) return undefined;
  return x >= area.left + area.width / 2 ? "right" : "left";
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
    /** Puts `id` on `side` of the shown tab, as a drop on that half does. */
    place: (id: TabId, side: Side) => {
      const into = shown();
      if (into === undefined || into === id) return;
      setPair(side === "left" ? [id, into] : [into, id]);
    },
    /** Exchanges the halves; the divider stays where it is. */
    swap: () => {
      setPair((now) => (now ? [now[1], now[0]] : now));
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
