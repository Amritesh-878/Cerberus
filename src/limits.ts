import { invoke } from "@tauri-apps/api/core";

import type { Account, AccountId, Profiles } from "./accounts";
import { resetText } from "./board";
import type { Statuses } from "./statuses";
import type { TabId } from "./tree";

/** What a limit popup offers for one chat: the account that hit, when it resets, and where the chat can go. */
export interface LimitHit {
  tab: TabId;
  account: AccountId;
  /** Unix seconds; `null` when the limit record gave no time. */
  resetsAt: number | null;
  offers: AccountId[];
  /** `clock` from the chat's limit record, which ends when the chat leaves `limited`; `meter` from a full 5-hour window. */
  source: "clock" | "meter";
}

/** The text typed into a chat moved off a limit, so its request carries on once the user presses Enter. */
export const AFTER_LIMIT = "continue";

/** The core's accounts below the warning line for a chat on `account`. */
export function limitOffers(account: AccountId): Promise<AccountId[]> {
  return invoke<AccountId[]>("live_offers", { account });
}

/** Offered accounts that are signed in now, in the core's order; only these can take the chat. */
export function signedInOffers(
  offers: readonly AccountId[],
  profiles: Profiles,
  statuses: Statuses,
): Account[] {
  return offers
    .map((id) => profiles.accounts.find((a) => a.id === id))
    .filter(
      (a): a is Account =>
        a !== undefined && statuses.get(a.id)?.state === "loggedIn",
    );
}

/** "work is limited until 6:40pm." */
export function limitText(
  account: string,
  resetsAt: number | null,
  locale?: string,
): string {
  return resetsAt === null
    ? `${account} has hit its limit.`
    : `${account} is limited until ${resetText(resetsAt, locale)}.`;
}

/** Whether a told window and a new hit's are one: equal times, or an open-ended clock window and a time still ahead. */
function sameWindow(
  told: number | null,
  resetsAt: number | null,
  now: number,
): boolean {
  if (told === resetsAt) return true;
  const known = told ?? resetsAt;
  return (told === null || resetsAt === null) && known !== null && known > now;
}

function sameOffers(a: readonly AccountId[], b: readonly AccountId[]): boolean {
  return a.length === b.length && a.every((id, i) => id === b[i]);
}

/** Limit popups waiting to show, each chat's at most once per limit window. */
export class LimitQueue {
  // Each chat's told windows by reset time; `null` is a clock record without one, until its limit clears.
  private readonly told = new Map<TabId, (number | null)[]>();
  private waiting: LimitHit[] = [];

  /** Queues the hit, or refreshes the offers of its window's waiting popup; true when the queue changed. */
  hit(hit: LimitHit, now = Date.now() / 1000): boolean {
    const told = this.told.get(hit.tab) ?? [];
    if (told.some((w) => sameWindow(w, hit.resetsAt, now))) {
      const at = this.waiting.findIndex(
        (h) => h.tab === hit.tab && sameWindow(h.resetsAt, hit.resetsAt, now),
      );
      const waiting = this.waiting[at];
      if (!waiting || sameOffers(waiting.offers, hit.offers)) return false;
      // An account that crossed the line since is no longer offered; the popup is not shown again.
      const fresh = {
        ...waiting,
        offers: hit.offers,
        resetsAt: waiting.resetsAt ?? hit.resetsAt,
      };
      this.waiting = this.waiting.map((h, i) => (i === at ? fresh : h));
      return true;
    }
    this.told.set(hit.tab, [...told, hit.resetsAt]);
    this.waiting.push(hit);
    return true;
  }

  /** The popup to show now. */
  first(): LimitHit | undefined {
    return this.waiting[0];
  }

  /** The shown popup was answered or dismissed. */
  done(): void {
    this.waiting = this.waiting.slice(1);
  }

  /** A closed tab's waiting popups go; its told windows stay, so a reopened id cannot repeat them. */
  forget(tab: TabId): void {
    this.waiting = this.waiting.filter((h) => h.tab !== tab);
  }

  /** The chat left `limited`: its clock popups go, and a later window without a reset time can be told. */
  cleared(tab: TabId): void {
    const told = this.told.get(tab);
    if (told)
      this.told.set(
        tab,
        told.filter((w) => w !== null),
      );
    // A full meter still stands while the chat is idle, so its popup stays.
    this.waiting = this.waiting.filter(
      (h) => h.tab !== tab || h.source !== "clock",
    );
  }
}
