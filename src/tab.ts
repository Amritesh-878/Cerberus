import type { Account, AccountId, Profiles } from "./accounts";
import type { TabKind } from "./keys";
import type { ProgramChoice, SpawnSpec } from "./pty";

export interface TabAccount {
  id: AccountId;
  name: string;
}

export interface Tab {
  /** The program id from `listPrograms`. */
  program: string;
  label: string;
  spec: SpawnSpec;
  imagePasteKey: string | null;
  account: TabAccount | null;
  /** Typed into the program once it starts, never followed by Enter. */
  input: string | null;
  /** Opened to sign the account in, so the tab shows how to finish. */
  signIn: boolean;
}

export interface TabOptions {
  account: Account | null;
  cwd: string | null;
  input?: string;
  signIn?: boolean;
}

/** Antigravity CLI and Codex CLI: plain programs with no Claude account and none of the Claude features. */
export const OTHER_CLIS = ["agy", "codex"] as const;

export function isOtherCli(program: string): boolean {
  return (OTHER_CLIS as readonly string[]).includes(program);
}

/** How the terminal treats keys: an ended Claude tab runs a shell. */
export function tabKind(program: string, ended: boolean): TabKind {
  if (isOtherCli(program)) return "cli";
  return program === "claude" && !ended ? "claude" : "shell";
}

export function tabFor(
  choice: ProgramChoice,
  options: TabOptions,
): Tab | undefined {
  if (choice.program === null) return undefined;
  const account = isOtherCli(choice.id) ? null : options.account;
  return {
    program: choice.id,
    label: choice.label,
    spec: {
      program: choice.program,
      args: choice.args,
      cwd: options.cwd,
      account: account?.id ?? null,
      dirs: [],
    },
    imagePasteKey: choice.imagePasteKey,
    account: account && { id: account.id, name: account.name },
    input: options.input ?? null,
    signIn: options.signIn ?? false,
  };
}

export function installed(
  programs: readonly ProgramChoice[],
  id: string,
): ProgramChoice | undefined {
  return programs.find((p) => p.id === id && p.program !== null);
}

export function shellChoice(
  profiles: Profiles,
  programs: readonly ProgramChoice[],
): ProgramChoice | undefined {
  const saved =
    profiles.shell === null ? undefined : installed(programs, profiles.shell);
  return saved ?? programs.find((p) => p.isShell && p.program !== null);
}
