import {
  createEffect,
  createResource,
  createSignal,
  Match,
  on,
  Show,
  Switch,
  untrack,
} from "solid-js";

import { loadProfiles, type Profiles, resetProfiles } from "./accounts";
import { Launcher } from "./Launcher";
import { listPrograms } from "./pty";
import { createStatuses } from "./statuses";
import type { Tab } from "./tab";
import { TerminalView } from "./Terminal";
import { Walkthrough } from "./Walkthrough";
import type { Step } from "./steps";

export function App() {
  const [setup, setSetup] = createSignal<Step>();
  // Signals, not a resource: an errored resource keeps throwing after a fresh value is set.
  const [profiles, setProfiles] = createSignal<Profiles>();
  const [loadError, setLoadError] = createSignal<string>();
  const loaded = (fresh: Profiles) => {
    if (fresh.accounts.length === 0) setSetup("install");
    setLoadError(undefined);
    setProfiles(fresh);
  };
  const fail = (error: unknown) => {
    setLoadError(String(error));
  };
  loadProfiles().then(loaded, fail);
  const [programs, { refetch: recheckPrograms }] = createResource(listPrograms);
  // Solid types a resource error as `any`.
  const programsError = (): unknown => programs.error;
  const [tab, setTab] = createSignal<Tab>();
  const [tabsClosed, setTabsClosed] = createSignal(0);
  const statuses = createStatuses();

  // Details refresh when the accounts or launcher screen opens and after a tab closes.
  createEffect(
    on(
      [() => profiles() === undefined, () => setup() === undefined, tabsClosed],
      () => {
        const ids = untrack(profiles)?.accounts.map((a) => a.id) ?? [];
        statuses.refresh(ids);
      },
    ),
  );

  const startFresh = () => {
    resetProfiles().then(loaded, fail);
  };

  const closeTab = () => {
    setTab(undefined);
    setTabsClosed((n) => n + 1);
    void recheckPrograms();
  };

  return (
    <>
      <Switch>
        <Match when={loadError()}>
          <main class="launcher">
            <h1>Cerberus</h1>
            <p class="error">{loadError()}</p>
            <p>
              Start fresh to set up your accounts again. The old file is kept
              beside the new one as <code>profiles.json.bad</code>.
            </p>
            <button type="button" onClick={startFresh}>
              Start fresh
            </button>
          </main>
        </Match>
        <Match when={profiles()}>
          {(loaded) => (
            <div class="home" hidden={tab() !== undefined}>
              <Show
                when={setup()}
                fallback={
                  <Launcher
                    profiles={loaded()}
                    programs={programs.latest}
                    programsError={programsError()}
                    statuses={statuses}
                    onLaunch={setTab}
                    onAccounts={() => setSetup("accounts")}
                  />
                }
              >
                {(start) => (
                  <Walkthrough
                    start={start()}
                    profiles={loaded()}
                    programs={programs.latest ?? []}
                    statuses={statuses}
                    onProfiles={setProfiles}
                    onOpen={setTab}
                    onRecheckPrograms={() => void recheckPrograms()}
                    onFinish={() => {
                      setSetup(undefined);
                    }}
                  />
                )}
              </Show>
            </div>
          )}
        </Match>
      </Switch>
      <Show when={tab()}>
        {(open) => (
          <section class="tab">
            <header class="tab-bar">
              <span>
                {open().label}
                <Show when={open().account}>
                  {(account) => (
                    <span class="tab-account" title={account().summary}>
                      {" · "}
                      {account().name}
                    </span>
                  )}
                </Show>
              </span>
              <button type="button" aria-label="Close tab" onClick={closeTab}>
                ×
              </button>
            </header>
            <TerminalView
              spec={open().spec}
              pastesImages={open().pastesImages}
              input={open().input}
            />
          </section>
        )}
      </Show>
    </>
  );
}
