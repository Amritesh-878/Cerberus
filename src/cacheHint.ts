import { createSignal, onCleanup } from "solid-js";

import type { LiveState } from "./board";

/** On a subscription the prompt cache lasts about an hour, so a reply after that rereads the whole chat. */
export const CACHE_IDLE_MS = 60 * 60 * 1000;
/** Below this the reread is cheap enough to say nothing. */
export const BIG_CONTEXT = 200_000;

/** The hint for a chat idle at its prompt since `idleSince`, or `undefined` when there is nothing to say. */
export function cacheHint(
  idleSince: number | undefined,
  tokens: number | null | undefined,
  now: number,
): string | undefined {
  if (idleSince === undefined || tokens === null || tokens === undefined)
    return undefined;
  if (now - idleSince < CACHE_IDLE_MS || tokens < BIG_CONTEXT) return undefined;
  const thousands = Math.round(tokens / 1000);
  return `Idle for over an hour: the next reply rereads ${String(thousands)}k tokens.`;
}

/** Each chat's time idle at its prompt, until a turn starts or the user types, with a clock for the hint. */
export function createCacheHints() {
  const [idleSince, setIdleSince] = createSignal<ReadonlyMap<number, number>>(
    new Map(),
  );
  const [now, setNow] = createSignal(Date.now());
  const timer = setInterval(() => setNow(Date.now()), 60_000);
  onCleanup(() => {
    clearInterval(timer);
  });
  const forget = (tab: number) => {
    if (!idleSince().has(tab)) return;
    setIdleSince((all) => {
      const next = new Map(all);
      next.delete(tab);
      return next;
    });
  };
  return {
    /** A state report: idle starts the clock once, anything else stops it. */
    reported: (tab: number, state: LiveState) => {
      if (state.state !== "idle") {
        forget(tab);
        return;
      }
      if (idleSince().has(tab)) return;
      const at = Date.now();
      setIdleSince((all) => new Map(all).set(tab, at));
    },
    /** The user typed in the tab, so the hint goes until the chat is next idle. */
    typed: forget,
    hint: (tab: number | undefined, tokens: number | null | undefined) =>
      tab === undefined
        ? undefined
        : cacheHint(idleSince().get(tab), tokens, now()),
  };
}
