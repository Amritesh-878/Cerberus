import type { AccountId, Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";

export type Step = "install" | "accounts" | "defaults";

export const STEPS: readonly Step[] = ["install", "accounts", "defaults"];

export const STEP_TITLES: Record<Step, string> = {
  install: "Claude Code",
  accounts: "Accounts",
  defaults: "Defaults",
};

export const INSTALL_COMMAND = "irm https://claude.ai/install.ps1 | iex";

export interface DefaultsDraft {
  account: AccountId | null;
  program: string | null;
  cwd: string;
}

export function nextStep(step: Step): Step | undefined {
  return STEPS[STEPS.indexOf(step) + 1];
}

export function previousStep(step: Step): Step | undefined {
  return STEPS[STEPS.indexOf(step) - 1];
}

export function installed(
  programs: readonly ProgramChoice[],
  id: string,
): ProgramChoice | undefined {
  return programs.find((p) => p.id === id && p.program !== null);
}

/** The saved default while it is installed, else Claude Code, else any installed program. */
export function preselectedProgram(
  profiles: Profiles,
  programs: readonly ProgramChoice[],
): string | null {
  const saved = profiles.defaultProgram;
  const pick =
    (saved === null ? undefined : installed(programs, saved)) ??
    installed(programs, "claude") ??
    programs.find((p) => p.program !== null);
  return pick?.id ?? null;
}

/** Why "Finish" is not allowed yet, or `undefined` when it is. */
export function finishBlocker(
  profiles: Profiles,
  draft: DefaultsDraft,
  programs: readonly ProgramChoice[],
): string | undefined {
  if (profiles.accounts.length === 0) return "Add at least one account.";
  if (!profiles.accounts.some((a) => a.id === draft.account)) {
    return "Pick a default account.";
  }
  if (draft.program === null || !installed(programs, draft.program)) {
    return "Pick a default program.";
  }
  return undefined;
}
