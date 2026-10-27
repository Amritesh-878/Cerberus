import { expect, test } from "vitest";

import { tabFor } from "./tab";

test("an installed program opens a tab in the home folder", () => {
  const tab = tabFor({
    id: "git-bash",
    label: "Git Bash",
    program: String.raw`C:\Git\bin\bash.exe`,
    args: ["--login", "-i"],
  });

  expect(tab).toEqual({
    label: "Git Bash",
    spec: {
      program: String.raw`C:\Git\bin\bash.exe`,
      args: ["--login", "-i"],
      cwd: null,
    },
  });
});

test("a missing program opens no tab", () => {
  expect(
    tabFor({ id: "claude", label: "Claude Code", program: null, args: [] }),
  ).toBeUndefined();
});
