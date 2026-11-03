import { afterEach, expect, test } from "vitest";

import type { Profiles } from "./accounts";
import { cleanup, click, field, renderApp, SAVED, settle } from "./harness";

afterEach(cleanup);

async function suggestionsBox(root: HTMLElement): Promise<HTMLInputElement> {
  click(root, "Accounts");
  await settle();
  const box = root.querySelector<HTMLInputElement>(
    ".suggestions-setting input",
  );
  if (!box) throw new Error("no suggestions box");
  return box;
}

test("suggestions start on and a click turns them off in the profiles file", async () => {
  const saves: unknown[] = [];
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    set_suggestions: (args: unknown) => {
      saves.push(field(args, "on"));
      saved = {
        ...saved,
        suggestions: field(args, "on") === true,
        revision: saves.length,
      };
      return saved;
    },
  });
  const box = await suggestionsBox(root);
  expect(box.checked).toBe(true);

  box.click();
  await settle();

  expect(saves).toEqual([false]);
  expect(saved.suggestions).toBe(false);
  expect(box.checked).toBe(false);
});

test("two quick clicks on the suggestions box are saved in order", async () => {
  const saves: unknown[] = [];
  const pending: (() => void)[] = [];
  let saved: Profiles = SAVED;
  const root = await renderApp(() => saved, {
    set_suggestions: (args: unknown) =>
      new Promise<Profiles>((resolve) => {
        saves.push(field(args, "on"));
        pending.push(() => {
          saved = {
            ...saved,
            suggestions: field(args, "on") === true,
            revision: saves.length,
          };
          resolve(saved);
        });
      }),
  });
  const box = await suggestionsBox(root);

  box.click();
  box.click();
  await settle();
  expect(saves).toEqual([false]);

  pending.shift()?.();
  await settle();
  expect(saves).toEqual([false, true]);
  expect(box.checked).toBe(true);
  pending.shift()?.();
  await settle();

  expect(saved.suggestions).toBe(true);
  expect(box.checked).toBe(true);
});

test("a failed save shows why and puts the box back", async () => {
  const root = await renderApp(() => SAVED, {
    set_suggestions: () => {
      throw new Error("disk full");
    },
  });
  const box = await suggestionsBox(root);

  box.click();
  await settle();

  expect(box.checked).toBe(true);
  const shown = root.querySelector(".suggestions-setting .error")?.textContent;
  expect(shown).toMatch(/^Suggestions setting not saved: .*disk full/);
});
