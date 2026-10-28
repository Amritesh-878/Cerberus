import { expect, test } from "vitest";

import { preselectedAccount, type Profiles, statusText } from "./accounts";

const PROFILES: Profiles = {
  accounts: [
    { id: 1, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 4, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 4,
  lastProgram: null,
  startFolder: null,
  revision: 0,
};

test("the launcher preselects the default account", () => {
  expect(preselectedAccount(PROFILES)).toBe(4);
});

test("without a saved default the first account is preselected", () => {
  expect(preselectedAccount({ ...PROFILES, defaultAccount: null })).toBe(1);
  expect(preselectedAccount({ ...PROFILES, defaultAccount: 9 })).toBe(1);
});

test("no accounts preselects none", () => {
  expect(
    preselectedAccount({ ...PROFILES, accounts: [], defaultAccount: null }),
  ).toBeNull();
});

test("a status still being checked says so", () => {
  expect(statusText(undefined)).toBe("checking…");
  expect(
    statusText({ state: "loggedIn", summary: "user@example.com · Pro" }),
  ).toBe("user@example.com · Pro");
});
