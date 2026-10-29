import { expect, test } from "vitest";

import type { Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import { shellChoice, tabFor } from "./tab";

const GIT_BASH = {
  id: "git-bash",
  label: "Git Bash",
  program: String.raw`C:\Git\bin\bash.exe`,
  args: ["--login", "-i"],
  pastesImages: false,
  isShell: true,
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
    program: "git-bash",
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
        isShell: false,
      },
      { account: null, cwd: null },
    ),
  ).toBeUndefined();
});

function program(id: string, path: string | null): ProgramChoice {
  return {
    id,
    label: id,
    program: path,
    args: [],
    pastesImages: false,
    isShell: id !== "claude",
  };
}

const PROGRAMS = [
  program("claude", String.raw`C:\bin\claude.exe`),
  program("powershell", String.raw`C:\ps\powershell.exe`),
  program("git-bash", null),
];

const PROFILES: Profiles = {
  accounts: [],
  defaultAccount: null,
  shell: null,
  startFolder: null,
  revision: 0,
};

test("the Shell button opens the saved shell, else the first one installed", () => {
  const bashInstalled = [
    ...PROGRAMS.slice(0, 2),
    program("git-bash", String.raw`C:\Git\bin\bash.exe`),
  ];
  const bash = { ...PROFILES, shell: "git-bash" };

  expect(shellChoice(bash, bashInstalled)?.id).toBe("git-bash");
  expect(shellChoice(bash, PROGRAMS)?.id).toBe("powershell");
  expect(shellChoice(PROFILES, PROGRAMS)?.id).toBe("powershell");
  expect(
    shellChoice(PROFILES, [
      program("claude", "c"),
      program("powershell", null),
    ]),
  ).toBeUndefined();
});
