import type { ProgramChoice, SpawnSpec } from "./pty";

export interface Tab {
  label: string;
  spec: SpawnSpec;
  pastesImages: boolean;
}

export function tabFor(choice: ProgramChoice): Tab | undefined {
  if (choice.program === null) return undefined;
  return {
    label: choice.label,
    spec: { program: choice.program, args: choice.args, cwd: null },
    pastesImages: choice.pastesImages,
  };
}
