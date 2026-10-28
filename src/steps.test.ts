import { expect, test } from "vitest";

import type { Profiles } from "./accounts";
import { finishBlocker, nextStep, previousStep } from "./steps";

const PROFILES: Profiles = {
  accounts: [{ id: 0, name: "personal", configDir: String.raw`C:\h\.claude` }],
  defaultAccount: 0,
  lastProgram: null,
  startFolder: null,
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
