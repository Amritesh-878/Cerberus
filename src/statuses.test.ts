import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, test } from "vitest";

import type { AuthStatus } from "./accounts";
import { createStatuses } from "./statuses";

afterEach(() => {
  clearMocks();
});

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

const LOGGED_OUT: AuthStatus = { state: "loggedOut", summary: "signed out" };
const LOGGED_IN: AuthStatus = {
  state: "loggedIn",
  summary: "user@example.com · Pro",
};

test("an older check that answers last does not replace a newer answer", async () => {
  const answers: ((status: AuthStatus) => void)[] = [];
  mockIPC(
    () =>
      new Promise<AuthStatus>((resolve) => {
        answers.push(resolve);
      }),
  );
  const statuses = createStatuses();

  statuses.refresh([1]);
  statuses.refresh([1]);
  await settled();
  answers[1]?.(LOGGED_IN);
  await settled();
  answers[0]?.(LOGGED_OUT);
  await settled();

  expect(statuses.get(1)).toEqual(LOGGED_IN);
});

test("a check shows as checking until it answers", async () => {
  mockIPC(() => LOGGED_IN);
  const statuses = createStatuses();

  statuses.refresh([1]);
  expect(statuses.get(1)).toBeUndefined();
  await settled();

  expect(statuses.get(1)).toEqual(LOGGED_IN);
});
