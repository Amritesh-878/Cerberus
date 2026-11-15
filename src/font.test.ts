import { expect, test } from "vitest";

import { terminalFont } from "./font";

test("macOS and Linux ask for their own monospace fonts, Windows keeps Cascadia Mono", () => {
  expect(terminalFont("mac")).toBe(
    'ui-monospace, "SF Mono", Menlo, Monaco, monospace',
  );
  expect(terminalFont("linux")).toBe(
    '"DejaVu Sans Mono", "Ubuntu Mono", "Liberation Mono", monospace',
  );
  expect(terminalFont("other")).toBe('"Cascadia Mono", Consolas, monospace');
});
