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
import { AccountDot } from "./icons";
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
  // Held apart from the account, since every saved change brings new account objects.
  const [draft, setDraft] = createSignal("");
  const [newName, setNewName] = createSignal("");
  const [newDir, setNewDir] = createSignal("");
  const [dirPicked, setDirPicked] = createSignal(false);
  // The account whose removal waits on a yes, with the tabs that use it.
  const [asking, setAsking] = createSignal<{ id: number; tabs: string[] }>();
  // Shown on the row it is about, since a refusal names that account.
  const [rowError, setRowError] = createSignal<{ id: number; text: string }>();

  const remove = (account: Account, confirmed: boolean) => {
    removeAccount(account.id, confirmed)
      .then((removal) => {
        setRowError(undefined);
        if (removal.kind === "inUse") {
          setAsking({ id: account.id, tabs: removal.tabs });
        } else {
          setAsking(undefined);
          props.onProfiles(removal.profiles);
        }
      })
      .catch((e: unknown) => {
        setAsking(undefined);
        setRowError({ id: account.id, text: String(e) });
      });
  };

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
      <Show when={props.profiles.accounts.length === 0}>
        <p class="accounts-empty">
          No accounts yet. Import one found on this computer, or add a new one
          below.
        </p>
      </Show>
      <ul class="accounts">
        <For each={props.profiles.accounts}>
          {(account) => (
            <li>
              <AccountDot id={account.id} />
              <Show
                when={editing() === account.id}
                fallback={
                  <span class="account-text">
                    <span class="account-line">
                      <strong>{account.name}</strong>
                      <span
                        class="account-status"
                        classList={{
                          out:
                            props.statuses.get(account.id)?.state ===
                            "loggedOut",
                        }}
                      >
                        {statusText(props.statuses.get(account.id))}
                      </span>
                    </span>
                    <span class="program">{account.configDir}</span>
                  </span>
                }
              >
                <form
                  onSubmit={(event) => {
                    event.preventDefault();
                    change(renameAccount(account.id, draft()), () => {
                      setEditing(undefined);
                    });
                  }}
                >
                  <input
                    name="name"
                    value={draft()}
                    onInput={(event) => setDraft(event.currentTarget.value)}
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
              <Show when={editing() !== account.id}>
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
                  <button
                    type="button"
                    onClick={() => {
                      setDraft(account.name);
                      setEditing(account.id);
                    }}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    title="Cerberus forgets this account. Its folder stays on disk."
                    onClick={() => {
                      remove(account, false);
                    }}
                  >
                    Remove
                  </button>
                </span>
              </Show>
              <Show when={asking()?.id === account.id && asking()}>
                {(ask) => (
                  <div class="row-confirm" role="alertdialog">
                    <p>
                      {ask().tabs.length === 1
                        ? "1 tab uses"
                        : `${String(ask().tabs.length)} tabs use`}{" "}
                      {account.name}: {ask().tabs.join(", ")}.
                    </p>
                    <p class="hint">
                      Removing it closes their programs. The tabs stay, so each
                      chat can continue on another account. Nothing is deleted.
                    </p>
                    <span class="row-confirm-actions">
                      <button
                        type="button"
                        class="danger"
                        onClick={() => {
                          remove(account, true);
                        }}
                      >
                        Remove {account.name}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setAsking(undefined);
                        }}
                      >
                        Cancel
                      </button>
                    </span>
                  </div>
                )}
              </Show>
              <Show when={rowError()?.id === account.id && rowError()}>
                {(refused) => (
                  <p class="row-error error" role="alert">
                    {refused().text}
                  </p>
                )}
              </Show>
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
                <span class="account-text">
                  <strong>{candidate.name}</strong>
                  <span class="program">{candidate.configDir}</span>
                </span>
                <span class="actions">
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
                </span>
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
        <button type="submit" class="go" disabled={newName().trim() === ""}>
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
