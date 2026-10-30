import { expect, test } from "vitest";

import type { Account, AuthStatus } from "./accounts";
import { continueTargets, limitShortcut, type Target } from "./carry";
import type { TreeTab } from "./tree";

const ACCOUNTS: Account[] = [
  { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
  { id: 1, name: "work", configDir: String.raw`C:\h\.claude-work` },
  { id: 2, name: "spare", configDir: String.raw`C:\h\.claude-spare` },
];

function tab(
  program: string,
  account: number | null,
  projectId: number | null = 1,
): TreeTab {
  return {
    id: 5,
    projectId,
    parentId: null,
    title: "a tab",
    launch: {
      program,
      label: program,
      spec: { program, args: [], cwd: null, account },
      pastesImages: false,
      account: account === null ? null : { id: account, name: "x" },
      input: null,
      signIn: false,
    },
  };
}

const signedIn = (): AuthStatus => ({ state: "loggedIn", summary: "fake" });

test("a Claude tab is offered every account but its own", () => {
  const targets = continueTargets(tab("claude", 1), ACCOUNTS, signedIn);

  expect(targets.map((t) => t.account.name)).toEqual(["personal", "spare"]);
});

test("a signed-out or unchecked account is listed but not signed in", () => {
  const status = (id: number): AuthStatus | undefined =>
    id === 0 ? { state: "loggedOut", summary: "fake" } : undefined;

  const targets = continueTargets(tab("claude", 1), ACCOUNTS, status);

  expect(targets.map((t) => t.signedIn)).toEqual([false, false]);
});

test("shells and setup tabs offer nothing", () => {
  expect(continueTargets(tab("powershell", 1), ACCOUNTS, signedIn)).toEqual([]);
  expect(continueTargets(tab("claude", 1, null), ACCOUNTS, signedIn)).toEqual(
    [],
  );
});

function targets(...signed: boolean[]): Target[] {
  return ACCOUNTS.slice(0, signed.length).map((account, i) => ({
    account,
    signedIn: signed[i] ?? false,
  }));
}

test("a limited row offers the first signed-in account", () => {
  const limited = { kind: "limited", resetsAt: 1_800_000_000 } as const;

  expect(limitShortcut(limited, targets(false, true, true))?.name).toBe("work");
  expect(limitShortcut(limited, targets(false, false))).toBeUndefined();
});

test("a row that is not limited offers no shortcut", () => {
  expect(limitShortcut({ kind: "waiting" }, targets(true))).toBeUndefined();
  expect(limitShortcut(undefined, targets(true))).toBeUndefined();
});
