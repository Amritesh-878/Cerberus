import {
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  untrack,
} from "solid-js";

import type { AccountId } from "./accounts";
import {
  type Changes,
  type Connectors,
  panelChanges,
  panelConnectors,
  panelPr,
  panelSkills,
  type PrRow,
  type Skill,
} from "./panelData";

/** What the panel reads for: the shown tab's folder and account. */
export interface Target {
  /** Only marks a tab switch, so two tabs on one folder still reload its files. */
  tab: number;
  folder: string | null;
  account: AccountId | null;
}

/** `undefined` while loading. */
export type Loaded<T> = { value: T } | { error: string } | undefined;

/** Measured at 0.2 to 0.7 s of git per poll on Windows; the core holds `gh` to once a minute per repo. */
export const POLL_MS = 10_000;

interface Slot<T> {
  get: () => Loaded<T>;
  /** Resolves to the value only if it was the newest ask and is still for the tab on screen. */
  load: (key: string, ask: () => Promise<T>) => Promise<T | undefined>;
  clear: () => void;
}

// A reply counts only if it is the newest ask and still for the tab on screen.
function slot<T>(current: () => string | undefined): Slot<T> {
  const [state, setState] = createSignal<{ key: string; loaded: Loaded<T> }>();
  let asked = 0;
  return {
    get: () => {
      const now = state();
      return now?.key === current() ? now?.loaded : undefined;
    },
    load: (key, ask) => {
      const n = ++asked;
      const fresh = () => n === asked && key === untrack(current);
      return ask().then(
        (value) => {
          if (!fresh()) return undefined;
          setState({ key, loaded: { value } });
          return value;
        },
        (error: unknown) => {
          if (fresh()) setState({ key, loaded: { error: String(error) } });
          return undefined;
        },
      );
    },
    clear: () => {
      asked += 1;
      setState(undefined);
    },
  };
}

export interface PanelFeed {
  changes: () => Loaded<Changes>;
  pr: () => Loaded<PrRow | null>;
  connectors: () => Loaded<Connectors>;
  skills: () => Loaded<Skill[]>;
}

/** Reads on mount, on each tab change and every `every` ms while the window shows; nothing once disposed. */
export function createPanelFeed(
  target: () => Target | undefined,
  every = POLL_MS,
): PanelFeed {
  const keyOf = (t: Target) => JSON.stringify([t.tab, t.folder, t.account]);
  // A memo, so a rename or any other change to the tab object reads nothing again.
  const current = createMemo(() => {
    const t = target();
    return t === undefined ? undefined : keyOf(t);
  });
  const changes = slot<Changes>(current);
  const pr = slot<PrRow | null>(current);
  const connectors = slot<Connectors>(current);
  const skills = slot<Skill[]>(current);

  // `live` turns false once the tab changes or the panel closes, so a late reply starts no more reads.
  const readRepo = (key: string, folder: string, live: () => boolean) => {
    void changes
      .load(key, () => panelChanges(folder))
      .then((found) => {
        if (!live()) return;
        if (found?.kind === "repo" && found.branch !== null) {
          void pr.load(key, () => panelPr(folder));
        } else if (found !== undefined) {
          void pr.load(key, () => Promise.resolve(null));
        }
      });
  };

  createEffect(
    on(current, (key) => {
      const t = untrack(target);
      if (key === undefined || t === undefined) return;
      const folder = t.folder;
      let live = true;
      onCleanup(() => {
        live = false;
      });
      if (folder === null) {
        changes.clear();
        pr.clear();
      } else {
        readRepo(key, folder, () => live);
      }
      void connectors.load(key, () => panelConnectors(t.account, folder));
      void skills.load(key, () => panelSkills(t.account, folder));
      if (folder === null) return;
      // Only git can change under a running chat often enough to poll; config and skills reload on a tab change.
      const timer = setInterval(() => {
        if (!document.hidden) readRepo(key, folder, () => live);
      }, every);
      onCleanup(() => {
        clearInterval(timer);
      });
    }),
  );

  return {
    changes: changes.get,
    pr: pr.get,
    connectors: connectors.get,
    skills: skills.get,
  };
}
