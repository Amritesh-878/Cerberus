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
}

/** The text typed into a chat moved off a limit, so its request carries on once he presses Enter. */
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

/** Limit popups waiting to show, each chat's at most once per limit window. */
export class LimitQueue {
  private readonly seen = new Set<string>();
  private waiting: LimitHit[] = [];

  /** Queues the hit unless this chat already had a popup for this window; true when it was queued. */
  hit(hit: LimitHit): boolean {
    // A window's reset time keys it; a record without one counts as one window until the tab's limit clears.
    const key = `${String(hit.tab)}:${String(hit.resetsAt ?? "unknown")}`;
    if (this.seen.has(key)) return false;
    this.seen.add(key);
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

  /** A closed tab's waiting popups go; its seen windows stay, so a reopened id cannot repeat them. */
  forget(tab: TabId): void {
    this.waiting = this.waiting.filter((h) => h.tab !== tab);
  }

  /** A limit that ended lets the same chat be told again in a later window without a reset time. */
  cleared(tab: TabId): void {
    this.seen.delete(`${String(tab)}:unknown`);
  }
}
