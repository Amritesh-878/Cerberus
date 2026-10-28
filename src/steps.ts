import type { Profiles } from "./accounts";

export type Step = "install" | "accounts" | "defaults";

export const STEPS: readonly Step[] = ["install", "accounts", "defaults"];

export const STEP_TITLES: Record<Step, string> = {
  install: "Install",
  accounts: "Accounts",
  defaults: "New tabs",
};

export const INSTALL_COMMAND = "irm https://claude.ai/install.ps1 | iex";

export function nextStep(step: Step): Step | undefined {
  return STEPS[STEPS.indexOf(step) + 1];
}

export function previousStep(step: Step): Step | undefined {
  return STEPS[STEPS.indexOf(step) - 1];
}

export function finishBlocker(profiles: Profiles): string | undefined {
  return profiles.accounts.length === 0 ? "Add an account first." : undefined;
}
