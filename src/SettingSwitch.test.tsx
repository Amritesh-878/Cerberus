import { afterEach, expect, test } from "vitest";

import { cleanup, click, field, renderApp, SAVED, settle } from "./harness";

afterEach(cleanup);

const APPLIES = "Applies to tabs started after a change.";

test("each switch is a short name over a one-line description, and says once when it waits for new tabs", async () => {
  const root = await renderApp(() => SAVED);
  click(root, "Accounts");
  await settle();

  const switches = [
    ...root.querySelectorAll<HTMLElement>(".setting-switch"),
  ].map((row) => ({
    name: row.querySelector(".setting-name")?.textContent,
    lines: [...row.querySelectorAll(".setting-description")].length,
    applies: row.textContent.split(APPLIES).length - 1,
  }));

  expect(switches).toEqual([
    { name: "Desktop alerts", lines: 1, applies: 0 },
    { name: "Suggestions from history", lines: 2, applies: 1 },
    { name: "Messages between accounts", lines: 2, applies: 1 },
  ]);
});

test("a click anywhere on the row flips the switch", async () => {
  const saves: unknown[] = [];
  const root = await renderApp(() => SAVED, {
    set_messages: (args: unknown) => {
      saves.push(field(args, "on"));
      return { ...SAVED, messages: field(args, "on") === true, revision: 1 };
    },
  });
  click(root, "Accounts");
  await settle();

  root
    .querySelector<HTMLElement>(".messages-setting .setting-description")
    ?.click();
  await settle();

  expect(saves).toEqual([false]);
});
