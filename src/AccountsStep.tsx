import { createResource, createSignal, For, Show } from "solid-js";

import {
  type Account,
  addAccount,
  detectAccounts,
  type Profiles,
  removeAccount,
  renameAccount,
  statusText,
  suggestConfigDir,
} from "./accounts";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { type Tab, tabFor } from "./tab";
import { installed } from "./steps";

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
  const [dirTyped, setDirTyped] = createSignal(false);

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

  const logIn = (account: Account) => {
    const claude = installed(props.programs, "claude");
    const tab = claude && tabFor(claude, { account, cwd: null });
    if (tab) props.onOpen(tab);
  };

  return (
    <section>
      <p>
        Each account is a Claude Code config folder with its own login, settings
        and chats.
      </p>
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
                <button
                  type="button"
                  disabled={!installed(props.programs, "claude")}
                  onClick={() => {
                    logIn(account);
                  }}
                >
                  Log in
                </button>
                <button type="button" onClick={() => setEditing(account.id)}>
                  Rename
                </button>
                <button
                  type="button"
                  title="Forget this account; its folder stays on disk"
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
          const dir = newDir().trim();
          // Enter can beat the suggestion, so an empty folder asks for it here.
          const folder =
            dir === "" ? suggestConfigDir(name) : Promise.resolve(dir);
          change(
            folder.then((configDir) => addAccount(name, configDir)),
            (profiles) => {
              setNewName("");
              setNewDir("");
              setDirTyped(false);
              added(profiles);
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
            suggestConfigDir(name)
              .then((dir) => {
                if (newName() === name && !dirTyped()) setNewDir(dir);
              })
              .catch((e: unknown) => {
                setError(String(e));
              });
          }}
        />
        <input
          placeholder="Folder"
          aria-label="Folder"
          value={newDir()}
          onInput={(event) => {
            setNewDir(event.currentTarget.value);
            setDirTyped(true);
          }}
        />
        <button type="submit" disabled={newName().trim() === ""}>
          Add
        </button>
      </form>
      <p class="hint">
        After adding, press Log in: Claude Code opens under that account; run
        /login there, then close the tab.
      </p>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
    </section>
  );
}
