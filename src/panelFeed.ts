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
  panelPrs,
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
  /** Extra folders, each read like the main one. */
  dirs: readonly string[];
}

/** One folder's changes, or why they could not be read. */
export interface FolderChanges {
  folder: string;
  changes: Changes | undefined;
  error: string | undefined;
}

/** A repo's PRs, read through the first of its folders. */
export interface RepoPrs {
  root: string;
  folder: string;
  name: string;
  prs: PrRow[];
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
  /** The main folder first, then each extra folder. */
  folders: () => Loaded<FolderChanges[]>;
  prs: () => Loaded<RepoPrs[]>;
  /** Reads the PR lists again, as after Ready for review; the core still asks GitHub once a minute. */
  rereadPrs: () => void;
  connectors: () => Loaded<Connectors>;
  skills: () => Loaded<Skill[]>;
}

/** Reads on mount, on each tab change and every `every` ms while the window shows; nothing once disposed. */
export function createPanelFeed(
  target: () => Target | undefined,
  every = POLL_MS,
  /** Changes whenever a chat links to the bridge or leaves it, which is when Cerberus's own server comes or goes. */
  links: () => unknown = () => undefined,
): PanelFeed {
  const keyOf = (t: Target) =>
    JSON.stringify([t.tab, t.folder, t.account, t.dirs]);
  // A memo, so a rename or any other change to the tab object reads nothing again.
  const current = createMemo(() => {
    const t = target();
    return t === undefined ? undefined : keyOf(t);
  });
  const folders = slot<FolderChanges[]>(current);
  const prs = slot<RepoPrs[]>(current);
  const connectors = slot<Connectors>(current);
  const skills = slot<Skill[]>(current);

  // `live` turns false once the tab changes or the panel closes, so a late reply starts no more reads.
  const readPrs = (key: string, found: readonly FolderChanges[]) => {
    const repos = new Map<string, { folder: string; name: string }>();
    for (const f of found) {
      if (
        f.changes?.kind === "repo" &&
        f.changes.gh &&
        !repos.has(f.changes.root)
      ) {
        repos.set(f.changes.root, { folder: f.folder, name: f.changes.name });
      }
    }
    void prs.load(key, () =>
      Promise.all(
        [...repos].map(([root, repo]) =>
          panelPrs(repo.folder).then(
            (list): RepoPrs => ({ root, ...repo, prs: list }),
            (): RepoPrs => ({ root, ...repo, prs: [] }),
          ),
        ),
      ),
    );
  };
  const readRepos = (
    key: string,
    paths: readonly string[],
    live: () => boolean,
  ) => {
    void folders
      .load(key, () =>
        Promise.all(
          paths.map((folder) =>
            panelChanges(folder).then(
              (changes): FolderChanges => ({
                folder,
                changes,
                error: undefined,
              }),
              (error: unknown): FolderChanges => ({
                folder,
                changes: undefined,
                error: String(error),
              }),
            ),
          ),
        ),
      )
      .then((found) => {
        if (live() && found !== undefined) readPrs(key, found);
      });
  };

  createEffect(
    on(current, (key) => {
      const t = untrack(target);
      if (key === undefined || t === undefined) return;
      const folder = t.folder;
      const paths = folder === null ? [] : [folder, ...t.dirs];
      let live = true;
      onCleanup(() => {
        live = false;
      });
      if (folder === null) {
        folders.clear();
        prs.clear();
      } else {
        readRepos(key, paths, () => live);
      }
      void connectors.load(key, () =>
        panelConnectors(t.account, folder, t.tab),
      );
      void skills.load(key, () => panelSkills(t.account, folder));
      if (folder === null) return;
      // Only git can change under a running chat often enough to poll; config and skills reload on a tab change.
      const timer = setInterval(() => {
        if (!document.hidden) readRepos(key, paths, () => live);
      }, every);
      onCleanup(() => {
        clearInterval(timer);
      });
    }),
  );

  // The link is made inside the chat's start, which can finish after the tab-change read above.
  createEffect(
    on(
      links,
      () => {
        const key = untrack(current);
        const t = untrack(target);
        if (key === undefined || t === undefined) return;
        void connectors.load(key, () =>
          panelConnectors(t.account, t.folder, t.tab),
        );
      },
      { defer: true },
    ),
  );

  return {
    folders: folders.get,
    prs: prs.get,
    rereadPrs: () => {
      const key = untrack(current);
      const found = untrack(folders.get);
      if (key !== undefined && found !== undefined && "value" in found) {
        readPrs(key, found.value);
      }
    },
    connectors: connectors.get,
    skills: skills.get,
  };
}
