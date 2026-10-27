import type { Account } from "./accounts";
import type { ProgramChoice, SpawnSpec } from "./pty";

export interface TabAccount {
  name: string;
  /** Email and plan, shown on hover. */
  summary: string;
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
  account: (Account & { summary: string }) | null;
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
    account: account && { name: account.name, summary: account.summary },
    input: options.input ?? null,
  };
}
