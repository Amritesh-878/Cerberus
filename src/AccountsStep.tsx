import { createResource, createSignal, For, Show } from "solid-js";

import {
  type Account,
  addAccount,
  detectAccounts,
  pickFolder,
  type Profiles,
  removeAccount,
  renameAccount,
  statusText,
  suggestConfigDir,
} from "./accounts";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { installed, type Tab, tabFor } from "./tab";

export function AccountsStep(props: {
  profiles: Profiles;
  programs: ProgramChoice[];
  statuses: Statuses;
  onProfiles: (profiles: Profiles) => void;
  onOpen: (tab: Tab) => void;
}) {
  const [error, setError] = createSignal<string>();
  const [detected] = createResource(() => props.profiles, detectAccounts);
  const [editing, setEditing] = createSignal<number>();
  const [newName, setNewName] = createSignal("");
  const [newDir, setNewDir] = createSignal("");
  const [dirPicked, setDirPicked] = createSignal(false);

  const change = (
    request: Promise<Profiles>,
    done?: (profiles: Profiles) => void,
  ) => {
    request
      .then((profiles) => {
        setError(undefined);
        props.onProfiles(profiles);
        done?.(profiles);
      })
      .catch((e: unknown) => {
        setError(String(e));
      });
  };

  const added = (profiles: Profiles) => {
    props.statuses.refresh(profiles.accounts.map((a) => a.id));
  };

  const signIn = (account: Account) => {
    const claude = installed(props.programs, "claude");
    const tab = claude && tabFor(claude, { account, cwd: null, signIn: true });
    if (tab) props.onOpen(tab);
  };

  const suggest = (name: string) => {
    suggestConfigDir(name)
      .then((dir) => {
        if (newName() === name && !dirPicked()) setNewDir(dir);
      })
      .catch((e: unknown) => {
        setError(String(e));
      });
  };

  const pick = () => {
    pickFolder()
      .then((dir) => {
        if (dir === null) return;
        setNewDir(dir);
        setDirPicked(true);
      })
      .catch((e: unknown) => {
        setError(String(e));
      });
  };

  return (
    <section>
      <p>Each account keeps its own sign-in, settings and chats.</p>
      <ul class="accounts">
        <For each={props.profiles.accounts}>
          {(account) => (
            <li>
              <Show
                when={editing() === account.id}
                fallback={<strong>{account.name}</strong>}
              >
                <form
                  onSubmit={(event) => {
                    event.preventDefault();
                    const field =
                      event.currentTarget.elements.namedItem("name");
                    if (field instanceof HTMLInputElement) {
                      change(renameAccount(account.id, field.value), () => {
                        setEditing(undefined);
                      });
                    }
                  }}
                >
                  <input
                    name="name"
                    value={account.name}
                    aria-label="New name"
                  />
                  <button type="submit">Save</button>
                  <button
                    type="button"
                    onClick={() => {
                      setEditing(undefined);
                    }}
                  >
                    Cancel
                  </button>
                </form>
              </Show>
              <span class="program">{account.configDir}</span>
              <span>{statusText(props.statuses.get(account.id))}</span>
              <span class="actions">
                <Show
                  when={
                    props.statuses.get(account.id)?.state === "loggedOut" &&
                    installed(props.programs, "claude")
                  }
                >
                  <button
                    type="button"
                    onClick={() => {
                      signIn(account);
                    }}
                  >
                    Sign in
                  </button>
                </Show>
                <button type="button" onClick={() => setEditing(account.id)}>
                  Rename
                </button>
                <button
                  type="button"
                  title="Cerberus forgets this account. Its folder stays on disk."
                  onClick={() => {
                    change(removeAccount(account.id));
                  }}
                >
                  Remove
                </button>
              </span>
            </li>
          )}
        </For>
      </ul>
      <Show when={detected()?.length}>
        <h3>Found on this computer</h3>
        <ul class="accounts">
          <For each={detected()}>
            {(candidate) => (
              <li>
                <strong>{candidate.name}</strong>
                <span class="program">{candidate.configDir}</span>
                <button
                  type="button"
                  onClick={() => {
                    change(
                      addAccount(candidate.name, candidate.configDir),
                      added,
                    );
                  }}
                >
                  Import
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <h3>Add an account</h3>
      <form
        class="add-account"
        onSubmit={(event) => {
          event.preventDefault();
          const name = newName();
          const dir = newDir();
          const before = new Set(props.profiles.accounts.map((a) => a.id));
          // Enter can beat the suggestion, so an empty folder asks for it here.
          const folder =
            dir === "" ? suggestConfigDir(name) : Promise.resolve(dir);
          change(
            folder.then((configDir) => addAccount(name, configDir)),
            (profiles) => {
              setNewName("");
              setNewDir("");
              setDirPicked(false);
              added(profiles);
              const fresh = profiles.accounts.find((a) => !before.has(a.id));
              if (fresh) signIn(fresh);
            },
          );
        }}
      >
        <input
          placeholder="Name"
          aria-label="Name"
          value={newName()}
          onInput={(event) => {
            const name = event.currentTarget.value;
            setNewName(name);
            // A folder suggested for the old name must not be submitted with the new one.
            if (!dirPicked()) setNewDir("");
            if (name.trim() !== "") suggest(name);
          }}
        />
        <button type="submit" disabled={newName().trim() === ""}>
          Add
        </button>
      </form>
      <Show when={newDir()}>
        {(dir) => (
          <p class="folder">
            Saved in <span class="program">{dir()}</span>{" "}
            <button type="button" onClick={pick}>
              Change…
            </button>
          </p>
        )}
      </Show>
      <p class="hint">
        Add opens Claude Code to sign this account in. Close the tab once you're
        signed in.
      </p>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
    </section>
  );
}
