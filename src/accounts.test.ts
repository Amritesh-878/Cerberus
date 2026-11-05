import { expect, test } from "vitest";

import {
  accountColor,
  PALETTE,
  preselectedAccount,
  type Profiles,
  savedColor,
  statusText,
} from "./accounts";

const PROFILES: Profiles = {
  accounts: [
    { id: 1, name: "personal", configDir: String.raw`C:\h\.claude` },
    { id: 4, name: "work", configDir: String.raw`C:\h\.claude-work` },
  ],
  defaultAccount: 4,
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

test("a dot takes the saved colour, else the one its id gave it before colours were saved", () => {
  expect(accountColor(6, "teal")).toBe("#4fb5a8");
  expect(accountColor(6, undefined)).toBe("#7aa2c8");
  expect(accountColor(6, "magenta")).toBe("#7aa2c8");
});

test("the palette keeps today's five colours first, in their old order", () => {
  expect(PALETTE.map((s) => s.hex).slice(0, 5)).toEqual([
    "#d97757",
    "#7aa2c8",
    "#7fb27f",
    "#b58ad6",
    "#d6b45a",
  ]);
  expect(PALETTE).toHaveLength(8);
});

test("a tab on a removed account still finds that account's colour", () => {
  const profiles: Profiles = {
    ...PROFILES,
    retired: [
      { id: 9, name: "old", configDir: String.raw`C:\h\.o`, color: "rose" },
    ],
  };

  expect(savedColor(profiles, 9)).toBe("rose");
  expect(savedColor(profiles, 4)).toBeUndefined();
});
