import type { Account, AccountId, Profiles } from "./accounts";
import type { ProgramChoice, SpawnSpec } from "./pty";

export interface TabAccount {
  id: AccountId;
  name: string;
}

export interface Tab {
  label: string;
  spec: SpawnSpec;
  pastesImages: boolean;
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

export function tabFor(
  choice: ProgramChoice,
  options: TabOptions,
): Tab | undefined {
  if (choice.program === null) return undefined;
  const account = options.account;
  return {
    label: choice.label,
    spec: {
      program: choice.program,
      args: choice.args,
      cwd: options.cwd,
      account: account?.id ?? null,
    },
    pastesImages: choice.pastesImages,
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

export function preselectedProgram(
  profiles: Profiles,
  programs: readonly ProgramChoice[],
): string | null {
  const last = profiles.lastProgram;
  const pick =
    (last === null ? undefined : installed(programs, last)) ??
    installed(programs, "claude") ??
    programs.find((p) => p.program !== null);
  return pick?.id ?? null;
}
