import { createSignal, onCleanup } from "solid-js";

import type { LiveState } from "./board";

/** When Claude Code doesn't say when the cache expires: on a subscription it lasts about an hour. */
export const CACHE_IDLE_MS = 60 * 60 * 1000;
/** Below this the reread is cheap enough to say nothing. */
export const BIG_CONTEXT = 200_000;

/** The hint for a chat idle at its prompt since `idleSince`, or `undefined` when there is nothing to say; `expiresAt` (Unix seconds) wins over the hour when known. */
export function cacheHint(
  idleSince: number | undefined,
  tokens: number | null | undefined,
  now: number,
  expiresAt: number | null | undefined = null,
): string | undefined {
  if (idleSince === undefined || tokens === null || tokens === undefined)
    return undefined;
  if (tokens < BIG_CONTEXT) return undefined;
  const thousands = `${String(Math.round(tokens / 1000))}k`;
  if (expiresAt !== null) {
    return now / 1000 >= expiresAt
      ? `The prompt cache has expired: the next reply rereads ${thousands} tokens.`
      : undefined;
  }
  if (now - idleSince < CACHE_IDLE_MS) return undefined;
  return `Idle for over an hour: the next reply rereads ${thousands} tokens.`;
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
    hint: (
      tab: number | undefined,
      tokens: number | null | undefined,
      expiresAt: number | null | undefined,
    ) =>
      tab === undefined
        ? undefined
        : cacheHint(idleSince().get(tab), tokens, now(), expiresAt),
  };
}
