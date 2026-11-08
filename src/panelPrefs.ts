import { createSignal, untrack } from "solid-js";

import {
  type AccountId,
  type PanelCard,
  type PanelPrefs,
  type Profiles,
  setPanel,
} from "./accounts";
import { PANEL_DEFAULT } from "./widths";

const CLOSED: PanelPrefs = {
  open: false,
  width: null,
  collapsed: [],
  expanded: [],
  favourites: {},
  dismissed: {},
};

/** Cards that start folded until the user opens them; the core keeps the same list. */
export const FOLDED_AT_FIRST: readonly PanelCard[] = ["connectors"];

function toggled<T>(list: readonly T[], item: T): T[] {
  return list.includes(item) ? list.filter((i) => i !== item) : [...list, item];
}

export interface PanelState {
  open: () => boolean;
  width: () => number;
  collapsed: (card: PanelCard) => boolean;
  /** The account's starred skill names. */
  favourites: (account: AccountId) => readonly string[];
  /** Hidden until the PR changes after it was dismissed. */
  isDismissed: (key: string, updated: string) => boolean;
  dismiss: (key: string, updated: string) => void;
  error: () => string | undefined;
  toggle: () => void;
  toggleCard: (card: PanelCard) => void;
  toggleFavourite: (account: AccountId, skill: string) => void;
  /** While dragging; `save` when the drag ends. */
  drag: (width: number) => void;
  save: () => void;
}

/** The panel's saved state, changed at once on screen and saved one write at a time, the latest last. */
export function createPanelState(
  profiles: () => Profiles | undefined,
  changed: (fresh: Profiles) => void,
): PanelState {
  // Ahead of the saved profiles while a change is on its way, so the panel follows the click at once.
  const [local, setLocal] = createSignal<PanelPrefs>();
  const [error, setError] = createSignal<string>();
  const prefs = () => local() ?? profiles()?.panel ?? CLOSED;
  let saving = false;
  let waiting = false;
  // Raised on every local change, so a reply never drops a change made after its save was sent.
  let edits = 0;
  const edit = (next: PanelPrefs) => {
    edits += 1;
    setLocal(next);
  };
  const send = () => {
    const next = untrack(prefs);
    const sent = edits;
    saving = true;
    setPanel(next)
      .then(
        (saved) => {
          setError(undefined);
          changed(saved);
          if (edits === sent) setLocal(undefined);
        },
        (failure: unknown) => {
          setError(String(failure));
          // A newer change gets its own try; with none, show what is saved.
          if (edits === sent) setLocal(undefined);
        },
      )
      .finally(() => {
        saving = false;
        if (waiting) {
          waiting = false;
          send();
        }
      });
  };
  const save = () => {
    if (saving) waiting = true;
    else send();
  };
  const change = (next: PanelPrefs) => {
    edit(next);
    save();
  };

  return {
    open: () => prefs().open,
    width: () => prefs().width ?? PANEL_DEFAULT,
    collapsed: (card) =>
      FOLDED_AT_FIRST.includes(card)
        ? !prefs().expanded.includes(card)
        : prefs().collapsed.includes(card),
    favourites: (account) => prefs().favourites[String(account)] ?? [],
    isDismissed: (key, updated) => prefs().dismissed?.[key] === updated,
    dismiss: (key, updated) => {
      const now = untrack(prefs);
      change({ ...now, dismissed: { ...now.dismissed, [key]: updated } });
    },
    error,
    toggle: () => {
      const now = untrack(prefs);
      change({ ...now, open: !now.open });
    },
    toggleCard: (card) => {
      const now = untrack(prefs);
      change(
        FOLDED_AT_FIRST.includes(card)
          ? { ...now, expanded: toggled(now.expanded, card) }
          : { ...now, collapsed: toggled(now.collapsed, card) },
      );
    },
    toggleFavourite: (account, skill) => {
      const now = untrack(prefs);
      const key = String(account);
      const favourites = {
        ...now.favourites,
        [key]: toggled(now.favourites[key] ?? [], skill),
      };
      change({ ...now, favourites });
    },
    drag: (width) => {
      edit({ ...untrack(prefs), width });
    },
    save,
  };
}
