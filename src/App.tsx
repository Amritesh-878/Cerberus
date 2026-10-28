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

import {
  loadProfiles,
  type Profiles,
  resetProfiles,
  statusText,
} from "./accounts";
import { AccountsScreen } from "./AccountsScreen";
import { Launcher } from "./Launcher";
import { listPrograms } from "./pty";
import { createStatuses } from "./statuses";
import type { Tab } from "./tab";
import { TerminalView } from "./Terminal";
import { Walkthrough } from "./Walkthrough";

type Screen = "walkthrough" | "accounts";

export function App() {
  const [screen, setScreen] = createSignal<Screen>();
  // Signals, not a resource: an errored resource keeps throwing after a fresh value is set.
  const [profiles, setProfiles] = createSignal<Profiles>();
  const [loadError, setLoadError] = createSignal<string>();
  const loaded = (fresh: Profiles) => {
    if (fresh.accounts.length === 0) setScreen("walkthrough");
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
    on([() => profiles() === undefined, screen, tabsClosed], () => {
      const ids = untrack(profiles)?.accounts.map((a) => a.id) ?? [];
      statuses.refresh(ids);
    }),
  );

  const startFresh = () => {
    resetProfiles().then(loaded, fail);
  };

  const toLauncher = () => {
    setScreen(undefined);
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
              beside the new one as <code>profiles.json.bad</code>, or{" "}
              <code>.bad2</code>, <code>.bad3</code> and so on when that name is
              taken.
            </p>
            <button type="button" onClick={startFresh}>
              Start fresh
            </button>
          </main>
        </Match>
        <Match when={profiles()}>
          {(loaded) => (
            <div class="home" hidden={tab() !== undefined}>
              <Switch
                fallback={
                  <Launcher
                    profiles={loaded()}
                    programs={programs.latest}
                    programsError={programsError()}
                    statuses={statuses}
                    onProfiles={setProfiles}
                    onLaunch={setTab}
                    onAccounts={() => setScreen("accounts")}
                  />
                }
              >
                <Match when={screen() === "walkthrough"}>
                  <Walkthrough
                    profiles={loaded()}
                    programs={programs.latest ?? []}
                    statuses={statuses}
                    onProfiles={setProfiles}
                    onOpen={setTab}
                    onRecheckPrograms={() => void recheckPrograms()}
                    onFinish={toLauncher}
                  />
                </Match>
                <Match when={screen() === "accounts"}>
                  <AccountsScreen
                    profiles={loaded()}
                    programs={programs.latest ?? []}
                    statuses={statuses}
                    onProfiles={setProfiles}
                    onOpen={setTab}
                    onClose={toLauncher}
                  />
                </Match>
              </Switch>
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
                    <span
                      class="tab-account"
                      title={statusText(statuses.get(account().id))}
                    >
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
            <Show when={open().signIn && open().account}>
              {(account) => (
                <div class="sign-in-bar">
                  <span>
                    Signing in {account().name}. Close this tab when you're
                    done.
                  </span>
                  <button type="button" onClick={closeTab}>
                    Done
                  </button>
                </div>
              )}
            </Show>
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
