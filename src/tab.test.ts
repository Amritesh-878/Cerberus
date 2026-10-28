import { expect, test } from "vitest";

import type { Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import { preselectedProgram, tabFor } from "./tab";

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
    account: { id: 2, name: "work" },
    input: null,
    signIn: false,
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

function program(id: string, path: string | null): ProgramChoice {
  return { id, label: id, program: path, args: [], pastesImages: false };
}

const PROGRAMS = [
  program("claude", String.raw`C:\bin\claude.exe`),
  program("powershell", String.raw`C:\ps\powershell.exe`),
  program("git-bash", null),
];

const PROFILES: Profiles = {
  accounts: [],
  defaultAccount: null,
  lastProgram: null,
  startFolder: null,
  revision: 0,
};

test("the launcher focuses the last program, then Claude Code, then any installed one", () => {
  const last = { ...PROFILES, lastProgram: "powershell" };
  const lastButGone = { ...PROFILES, lastProgram: "git-bash" };
  const noClaude = [program("claude", null), ...PROGRAMS.slice(1)];

  expect(preselectedProgram(last, PROGRAMS)).toBe("powershell");
  expect(preselectedProgram(lastButGone, PROGRAMS)).toBe("claude");
  expect(preselectedProgram(PROFILES, noClaude)).toBe("powershell");
  expect(preselectedProgram(PROFILES, [program("claude", null)])).toBeNull();
});
