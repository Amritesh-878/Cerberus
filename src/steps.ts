import type { Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import { installed, OTHER_CLIS } from "./tab";

export type Step = "install" | "accounts" | "defaults";

export const STEPS: readonly Step[] = ["install", "accounts", "defaults"];

export const STEP_TITLES: Record<Step, string> = {
  install: "Install",
  accounts: "Accounts",
  defaults: "New tabs",
};

export const STEP_HEADINGS: Record<Step, string> = {
  install: "Install Claude Code",
  accounts: "Add your accounts",
  defaults: "Pick the account for new tabs",
};

export const STEP_LEADS: Record<Step, string> = {
  install: "Every Claude Code tab runs the official claude program.",
  accounts: "Each account has its own sign-in, settings and chats.",
  defaults: "New tabs start on this account. You can pick another per tab.",
};

/** The AI CLI tabs run when Claude Code is missing; accounts are for Claude Code alone, so setup then skips them. */
export function otherCliOnly(
  programs: readonly ProgramChoice[] | undefined,
): ProgramChoice | undefined {
  if (!programs || installed(programs, "claude")) return undefined;
  return OTHER_CLIS.map((id) => installed(programs, id)).find(
    (choice) => choice !== undefined,
  );
}

export function stepsFor(programs: readonly ProgramChoice[]): readonly Step[] {
  return otherCliOnly(programs) ? ["install"] : STEPS;
}

export function nextStep(
  step: Step,
  steps: readonly Step[] = STEPS,
): Step | undefined {
  const at = steps.indexOf(step);
  return at < 0 ? undefined : steps[at + 1];
}

export function previousStep(
  step: Step,
  steps: readonly Step[] = STEPS,
): Step | undefined {
  const at = steps.indexOf(step);
  return at < 1 ? undefined : steps[at - 1];
}

export function finishBlocker(
  profiles: Profiles,
  programs: readonly ProgramChoice[] = [],
): string | undefined {
  if (otherCliOnly(programs)) return undefined;
  return profiles.accounts.length === 0 ? "Add an account first." : undefined;
}

/** What setup still lacks, so a skipped walkthrough can say so on the start page. */
export type Gap = "claude" | "account";

export const GAP_TEXT: Record<Gap, string> = {
  claude: "Claude Code isn't installed, so tabs can only run a shell.",
  account: "No account yet. Claude Code tabs use its default sign-in.",
};

/** Programs still loading count as found, so the start page does not flash a warning; a user of another AI CLI is not nagged. */
export function setupGaps(
  profiles: Profiles,
  programs: readonly ProgramChoice[] | undefined,
): Gap[] {
  if (otherCliOnly(programs)) return [];
  const gaps: Gap[] = [];
  if (programs && !installed(programs, "claude")) gaps.push("claude");
  if (profiles.accounts.length === 0) gaps.push("account");
  return gaps;
}

export function stepFor(gap: Gap): Step {
  return gap === "claude" ? "install" : "accounts";
}

/** A step is done once what it sets up exists; the last step is a choice, so it never shows done. */
export function stepDone(
  step: Step,
  profiles: Profiles,
  programs: readonly ProgramChoice[],
): boolean {
  if (step === "install") return installed(programs, "claude") !== undefined;
  if (step === "accounts") return profiles.accounts.length > 0;
  return false;
}
