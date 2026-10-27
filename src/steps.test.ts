import { expect, test } from "vitest";

import type { Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import {
  finishBlocker,
  nextStep,
  preselectedProgram,
  previousStep,
} from "./steps";

function program(id: string, program: string | null): ProgramChoice {
  return { id, label: id, program, args: [], pastesImages: false };
}

const PROGRAMS = [
  program("claude", String.raw`C:\bin\claude.exe`),
  program("powershell", String.raw`C:\ps\powershell.exe`),
  program("git-bash", null),
];

const PROFILES: Profiles = {
  accounts: [
    { id: 0, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 3, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 3,
  defaultProgram: null,
  defaultCwd: null,
};

const READY = { account: 3, program: "claude", cwd: "" };

test("steps run install, accounts, defaults", () => {
  expect(nextStep("install")).toBe("accounts");
  expect(nextStep("accounts")).toBe("defaults");
  expect(nextStep("defaults")).toBeUndefined();
  expect(previousStep("defaults")).toBe("accounts");
  expect(previousStep("install")).toBeUndefined();
});

test("finish is allowed with an account, a default account and an installed program", () => {
  expect(finishBlocker(PROFILES, READY, PROGRAMS)).toBeUndefined();
});

test("finish needs at least one account", () => {
  const none = { ...PROFILES, accounts: [], defaultAccount: null };

  expect(finishBlocker(none, READY, PROGRAMS)).toBe(
    "Add at least one account.",
  );
});

test("finish needs a default account that is saved", () => {
  expect(finishBlocker(PROFILES, { ...READY, account: null }, PROGRAMS)).toBe(
    "Pick a default account.",
  );
  expect(finishBlocker(PROFILES, { ...READY, account: 9 }, PROGRAMS)).toBe(
    "Pick a default account.",
  );
});

test("finish needs a default program that is installed", () => {
  expect(
    finishBlocker(PROFILES, { ...READY, program: "git-bash" }, PROGRAMS),
  ).toBe("Pick a default program.");
  expect(finishBlocker(PROFILES, { ...READY, program: null }, PROGRAMS)).toBe(
    "Pick a default program.",
  );
});

test("the program preselects the saved default, then Claude Code, then any installed one", () => {
  const saved = { ...PROFILES, defaultProgram: "powershell" };
  const savedButGone = { ...PROFILES, defaultProgram: "git-bash" };
  const noClaude = [program("claude", null), ...PROGRAMS.slice(1)];

  expect(preselectedProgram(saved, PROGRAMS)).toBe("powershell");
  expect(preselectedProgram(savedButGone, PROGRAMS)).toBe("claude");
  expect(preselectedProgram(PROFILES, noClaude)).toBe("powershell");
  expect(preselectedProgram(PROFILES, [program("claude", null)])).toBeNull();
});
