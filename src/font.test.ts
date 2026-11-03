import { expect, test } from "vitest";

import { terminalFont } from "./font";

test("macOS asks for its own monospace fonts, Windows keeps Cascadia Mono", () => {
  expect(terminalFont("mac")).toBe(
    'ui-monospace, "SF Mono", Menlo, Monaco, monospace',
  );
  expect(terminalFont("other")).toBe('"Cascadia Mono", Consolas, monospace');
});
