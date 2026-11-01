import { invoke } from "@tauri-apps/api/core";

import type { Account, AccountId, AuthStatus } from "./accounts";
import type { Mark } from "./board";
import type { TabId, TreeTab } from "./tree";

export interface Target {
  account: Account;
  /** Signed-out accounts are listed so the menu can say why they cannot be picked. */
  signedIn: boolean;
}

/** Accounts a tab's chat can move to: a project's Claude tab, to any account but its own. */
export function continueTargets(
  tab: TreeTab,
  accounts: readonly Account[],
  status: (id: AccountId) => AuthStatus | undefined,
): Target[] {
  if (tab.projectId === null || tab.launch.program !== "claude") return [];
  return accounts
    .filter((account) => account.id !== tab.launch.account?.id)
    .map((account) => ({
      account,
      signedIn: status(account.id)?.state === "loggedIn",
    }));
}

/** The account a limited tab's row offers: the first other account that is signed in. */
export function limitShortcut(
  mark: Mark | undefined,
  targets: readonly Target[],
): Account | undefined {
  if (mark?.kind !== "limited") return undefined;
  return targets.find((target) => target.signedIn)?.account;
}

export function continueOnAccount(
  tab: TabId,
  account: AccountId,
): Promise<void> {
  return invoke("continue_on_account", { tab, account });
}
