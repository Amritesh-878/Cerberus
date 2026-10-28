import { createSignal, For, Show } from "solid-js";

import { AccountsStep } from "./AccountsStep";
import {
  pickFolder,
  preselectedAccount,
  type Profiles,
  setDefaultAccount,
  setStartFolder,
} from "./accounts";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import type { Tab } from "./tab";

export function AccountsScreen(props: {
  profiles: Profiles;
  programs: ProgramChoice[];
  statuses: Statuses;
  onProfiles: (profiles: Profiles) => void;
  onOpen: (tab: Tab) => void;
  onClose: () => void;
}) {
  return (
    <main class="walkthrough">
      <h1>Accounts</h1>
      <AccountsStep
        profiles={props.profiles}
        programs={props.programs}
        statuses={props.statuses}
        onProfiles={props.onProfiles}
        onOpen={props.onOpen}
      />
      <h3>New tabs</h3>
      <section class="defaults">
        <DefaultAccountPicker
          profiles={props.profiles}
          onProfiles={props.onProfiles}
        />
        <StartFolderSetting
          profiles={props.profiles}
          onProfiles={props.onProfiles}
        />
      </section>
      <nav class="step-nav">
        <button
          type="button"
          onClick={() => {
            props.onClose();
          }}
        >
          Back to launcher
        </button>
      </nav>
    </main>
  );
}

export function DefaultAccountPicker(props: {
  profiles: Profiles;
  onProfiles: (profiles: Profiles) => void;
}) {
  const [error, setError] = createSignal<string>();

  return (
    <>
      <label>
        Which account should new tabs use?
        <select
          onChange={(event) => {
            setDefaultAccount(Number(event.currentTarget.value))
              .then((profiles) => {
                setError(undefined);
                props.onProfiles(profiles);
              })
              .catch((e: unknown) => {
                setError(String(e));
              });
          }}
        >
          <For each={props.profiles.accounts}>
            {(a) => (
              <option
                value={a.id}
                selected={a.id === preselectedAccount(props.profiles)}
              >
                {a.name}
              </option>
            )}
          </For>
        </select>
      </label>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
    </>
  );
}

function StartFolderSetting(props: {
  profiles: Profiles;
  onProfiles: (profiles: Profiles) => void;
}) {
  const [error, setError] = createSignal<string>();
  const save = (folder: string | null) => {
    setStartFolder(folder)
      .then((profiles) => {
        setError(undefined);
        props.onProfiles(profiles);
      })
      .catch((e: unknown) => {
        setError(String(e));
      });
  };

  return (
    <div class="start-folder">
      Start folder:{" "}
      <span class="program">{props.profiles.startFolder ?? "Home"}</span>{" "}
      <button
        type="button"
        onClick={() => {
          pickFolder()
            .then((dir) => {
              if (dir !== null) save(dir);
            })
            .catch((e: unknown) => {
              setError(String(e));
            });
        }}
      >
        Change…
      </button>
      <Show when={props.profiles.startFolder !== null}>
        {" "}
        <button
          type="button"
          onClick={() => {
            save(null);
          }}
        >
          Use Home
        </button>
      </Show>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
    </div>
  );
}
