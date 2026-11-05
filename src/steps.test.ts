import { expect, test } from "vitest";

import type { Profiles } from "./accounts";
import {
  finishBlocker,
  nextStep,
  previousStep,
  setupGaps,
  stepDone,
  stepFor,
} from "./steps";

const PROFILES: Profiles = {
  accounts: [{ id: 0, name: "personal", configDir: String.raw`C:\h\.claude` }],
  defaultAccount: 0,
  shell: null,
  startFolder: null,
  sidebarWidth: null,
  panel: { open: false, width: null, collapsed: [] },
  alerts: true,
  suggestions: true,
  retired: [],
  setup: null,
  revision: 0,
};

test("steps run install, accounts, defaults", () => {
  expect(nextStep("install")).toBe("accounts");
  expect(nextStep("accounts")).toBe("defaults");
  expect(nextStep("defaults")).toBeUndefined();
  expect(previousStep("defaults")).toBe("accounts");
  expect(previousStep("install")).toBeUndefined();
});

test("finish is allowed once an account exists", () => {
  expect(finishBlocker(PROFILES)).toBeUndefined();
});

test("finish needs at least one account", () => {
  const none = { ...PROFILES, accounts: [], defaultAccount: null };

  expect(finishBlocker(none)).toBe("Add an account first.");
});

const CLAUDE = {
  id: "claude",
  label: "Claude Code",
  program: String.raw`C:\bin\claude.exe`,
  args: [],
  imagePasteKey: null,
  isShell: false,
  installCommand: null,
  installShell: null,
};

test("a finished setup leaves nothing for the start page to say", () => {
  expect(setupGaps(PROFILES, [CLAUDE])).toEqual([]);
});

test("a skipped setup names what is missing, Claude Code first", () => {
  const none = { ...PROFILES, accounts: [], defaultAccount: null };

  expect(setupGaps(none, [{ ...CLAUDE, program: null }])).toEqual([
    "claude",
    "account",
  ]);
  expect(stepFor("claude")).toBe("install");
  expect(stepFor("account")).toBe("accounts");
});

test("while programs are still loading Claude Code is not reported missing", () => {
  expect(setupGaps(PROFILES, undefined)).toEqual([]);
});

test("a step shows done once what it sets up exists", () => {
  const none = { ...PROFILES, accounts: [], defaultAccount: null };

  expect(stepDone("install", PROFILES, [CLAUDE])).toBe(true);
  expect(stepDone("accounts", none, [CLAUDE])).toBe(false);
  expect(stepDone("defaults", PROFILES, [CLAUDE])).toBe(false);
});
