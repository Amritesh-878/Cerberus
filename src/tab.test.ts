import { expect, test } from "vitest";

import { tabFor } from "./tab";

const GIT_BASH = {
  id: "git-bash",
  label: "Git Bash",
  program: String.raw`C:\Git\bin\bash.exe`,
  args: ["--login", "-i"],
  pastesImages: false,
};

test("an installed program opens a tab under its account", () => {
  const tab = tabFor(GIT_BASH, {
    account: {
      id: 2,
      name: "work",
      configDir: String.raw`C:\h\.claude-work`,
      summary: "user@example.com · Team",
    },
    cwd: String.raw`C:\code`,
  });

  expect(tab).toEqual({
    label: "Git Bash",
    spec: {
      program: String.raw`C:\Git\bin\bash.exe`,
      args: ["--login", "-i"],
      cwd: String.raw`C:\code`,
      account: 2,
    },
    pastesImages: false,
    account: { name: "work", summary: "user@example.com · Team" },
    input: null,
  });
});

test("a tab with no account carries its typed input", () => {
  const tab = tabFor(GIT_BASH, { account: null, cwd: null, input: "ls" });

  expect(tab?.spec.account).toBeNull();
  expect(tab?.account).toBeNull();
  expect(tab?.input).toBe("ls");
});

test("a missing program opens no tab", () => {
  expect(
    tabFor(
      {
        id: "claude",
        label: "Claude Code",
        program: null,
        args: [],
        pastesImages: true,
      },
      { account: null, cwd: null },
    ),
  ).toBeUndefined();
});
