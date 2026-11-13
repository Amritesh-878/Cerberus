import { expect, test } from "vitest";

import { endedScreen } from "./endedScreen";

const NOTE = "\x1b[2m[the chat ended; this tab is now a shell]\x1b[0m\r\n";

test("the last frame goes, and the resume line and the note stay at the top", () => {
  const frame = [
    "> /exit",
    "  /exit   Exit the REPL",
    "  /export Export the conversation",
    "",
    "Resume this session with:",
    "claude --resume 0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00   ",
    "",
    "",
  ];

  expect(endedScreen(frame)).toBe(
    "\x1b[H\x1b[2JResume this session with:\r\nclaude --resume 0f6c3a52-7d1e-4b8a-9c2f-5e4d3b2a1f00\r\n" +
      NOTE,
  );
});

test("with no resume line, only the note is left", () => {
  expect(endedScreen(["> /exit", "  /exit   Exit the REPL", ""])).toBe(
    `\x1b[H\x1b[2J${NOTE}`,
  );
});
