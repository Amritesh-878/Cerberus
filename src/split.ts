import { createEffect, createSignal, on, untrack } from "solid-js";

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

/** Two chats side by side while one of them is the shown tab; showing any other tab closes it. */
export function createSplit(shown: () => TabId | undefined) {
  const [pair, setPair] = createSignal<Pair>();
  createEffect(
    on(shown, (id) => {
      const now = untrack(pair);
      if (now && (id === undefined || !now.includes(id))) setPair(undefined);
    }),
  );
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
    open: (from: TabId, to: TabId) => {
      setPair((now) => nextPair(now, from, to));
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
