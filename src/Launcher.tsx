import { createEffect, createSignal, For, on, Show, untrack } from "solid-js";

import {
  preselectedAccount,
  type Profiles,
  rememberProgram,
  statusText,
} from "./accounts";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { preselectedProgram, type Tab, tabFor } from "./tab";

export function Launcher(props: {
  profiles: Profiles;
  programs: ProgramChoice[] | undefined;
  programsError: unknown;
  statuses: Statuses;
  onProfiles: (profiles: Profiles) => void;
  onLaunch: (tab: Tab) => void;
  onAccounts: () => void;
}) {
  const [accountId, setAccountId] = createSignal(
    untrack(() => preselectedAccount(props.profiles)),
  );
  // A default saved after the accounts screen closed still has to reach the picker.
  createEffect(
    on(
      () => props.profiles.defaultAccount,
      () => setAccountId(preselectedAccount(props.profiles)),
      { defer: true },
    ),
  );
  const account = () =>
    props.profiles.accounts.find((a) => a.id === accountId());
  const summary = () => {
    const id = accountId();
    return id === null ? "" : statusText(props.statuses.get(id));
  };

  const launch = (choice: ProgramChoice) => {
    const chosen = account();
    const tab = tabFor(choice, {
      account: chosen ?? null,
      cwd: props.profiles.startFolder,
    });
    if (!tab) return;
    props.onLaunch(tab);
    rememberProgram(choice.id).then(props.onProfiles, (error: unknown) => {
      console.error("could not save the last program:", error);
    });
  };

  return (
    <main class="launcher">
      <h1>Cerberus</h1>
      <div class="account-picker">
        <label>
          Account{" "}
          <select
            onChange={(event) => {
              setAccountId(Number(event.currentTarget.value));
            }}
          >
            <For each={props.profiles.accounts}>
              {(a) => (
                <option value={a.id} selected={a.id === accountId()}>
                  {a.name}
                </option>
              )}
            </For>
          </select>
        </label>
        <span class="program">{summary()}</span>
        <button
          type="button"
          class="link"
          onClick={() => {
            props.onAccounts();
          }}
        >
          Accounts…
        </button>
      </div>
      <Show when={props.programsError !== undefined}>
        <p class="error">
          Could not list programs: {String(props.programsError)}
        </p>
      </Show>
      <ul>
        <For each={props.programs}>
          {(choice) => (
            <li>
              <button
                type="button"
                disabled={choice.program === null}
                ref={(el) => {
                  const focused = preselectedProgram(
                    props.profiles,
                    props.programs ?? [],
                  );
                  if (choice.id === focused) {
                    queueMicrotask(() => {
                      el.focus();
                    });
                  }
                }}
                onClick={() => {
                  launch(choice);
                }}
              >
                {choice.label}
              </button>
              <span class="program">{choice.program ?? "not installed"}</span>
            </li>
          )}
        </For>
      </ul>
    </main>
  );
}
