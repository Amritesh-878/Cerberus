import type { Account, AccountId } from "./accounts";
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
}

export interface TabOptions {
  account: Account | null;
  cwd: string | null;
  input?: string;
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
  };
}
