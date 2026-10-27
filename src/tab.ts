import type { ProgramChoice, SpawnSpec } from "./pty";

export interface Tab {
  label: string;
  spec: SpawnSpec;
}

/** The tab a launcher choice opens, or `undefined` when its program is not installed. */
export function tabFor(choice: ProgramChoice): Tab | undefined {
  if (choice.program === null) return undefined;
  return {
    label: choice.label,
    spec: { program: choice.program, args: choice.args, cwd: null },
  };
}
