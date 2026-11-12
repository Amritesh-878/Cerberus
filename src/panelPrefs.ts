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

/** The panel's built-in order, which a dragged order replaces until Reset. */
export const CARD_ORDER: readonly PanelCard[] = [
  "environment",
  "context",
  "outputs",
  "tasks",
  "usage",
  "changes",
  "skills",
  "connectors",
];

/** `saved` first, then any card it leaves out in the built-in order. */
export function fullOrder(saved: readonly PanelCard[]): PanelCard[] {
  const known = saved.filter(
    (card, i) => CARD_ORDER.includes(card) && saved.indexOf(card) === i,
  );
  return [...known, ...CARD_ORDER.filter((card) => !known.includes(card))];
}

/** The order after `card` moves in front of `before`, or to the end. */
export function movedCard(
  order: readonly PanelCard[],
  card: PanelCard,
  before: PanelCard | undefined,
): PanelCard[] {
  const rest = order.filter((c) => c !== card);
  const at = before === undefined ? -1 : rest.indexOf(before);
  return at < 0
    ? [...rest, card]
    : [...rest.slice(0, at), card, ...rest.slice(at)];
}

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
  /** The card's place in the panel, for its CSS order. */
  orderOf: (card: PanelCard) => number;
  /** The user dragged the cards out of the built-in order. */
  reordered: () => boolean;
  moveCard: (card: PanelCard, before: PanelCard | undefined) => void;
  resetOrder: () => void;
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
    orderOf: (card) => fullOrder(prefs().order ?? []).indexOf(card),
    reordered: () => (prefs().order ?? []).length > 0,
    moveCard: (card, before) => {
      const now = untrack(prefs);
      const next = movedCard(fullOrder(now.order ?? []), card, before);
      const same = next.every((c, i) => c === CARD_ORDER[i]);
      change({ ...now, order: same ? [] : next });
    },
    resetOrder: () => {
      change({ ...untrack(prefs), order: [] });
    },
    drag: (width) => {
      edit({ ...untrack(prefs), width });
    },
    save,
  };
}
