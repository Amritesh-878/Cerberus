import { createResource, createSignal, For, Show } from "solid-js";

import {
  type Account,
  type AccountId,
  accountLogins,
  addAccount,
  chatDeletion,
  crossesOrganization,
  deletionText,
  detectAccounts,
  keepChats,
  OTHER_ORGANIZATION,
  pickFolder,
  preselectedAccount,
  type Profiles,
  removeAccount,
  renameAccount,
  setAccountColor,
  statusText,
  suggestConfigDir,
  swatchOf,
} from "./accounts";
import { ColorPicker } from "./ColorPicker";
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
  /** Moves every tab on an account to another; absent where no tabs can exist yet. */
  onMoveTabs?: (from: AccountId, to: Account) => Promise<void>;
}) {
  const [error, setError] = createSignal<string>();
  const [detected] = createResource(() => props.profiles, detectAccounts);
  // Read again with every change, since a removal or sign-in changes who shares a login.
  const [logins] = createResource(() => props.profiles, accountLogins);
  const twinOf = (account: Account) => {
    const first = logins()?.find((l) => l.id === account.id)?.sameAs;
    return props.profiles.accounts.find((a) => a.id === first);
  };
  const [editing, setEditing] = createSignal<number>();
  // Held apart from the account, since every saved change brings new account objects.
  const [draft, setDraft] = createSignal("");
  const [newName, setNewName] = createSignal("");
  const [newDir, setNewDir] = createSignal("");
  const [dirPicked, setDirPicked] = createSignal(false);
  // The account whose colour swatches are open.
  const [picking, setPicking] = createSignal<number>();
  // The account whose removal waits on a yes, with the tabs that use it.
  const [asking, setAsking] = createSignal<{ id: number; tabs: string[] }>();
  // Shown on the row it is about, since a refusal names that account.
  const [rowError, setRowError] = createSignal<{ id: number; text: string }>();
  const [deletion, { mutate: setDeletion }] = createResource(
    () => props.profiles,
    chatDeletion,
  );
  const deletesOf = (account: Account) =>
    deletion()?.find((d) => d.id === account.id);
  const keep = (account: Account) => {
    keepChats(account.id)
      .then((left) => {
        setRowError(undefined);
        setDeletion(left);
      })
      .catch((e: unknown) => {
        setRowError({ id: account.id, text: String(e) });
      });
  };
  // Where the removed account's tabs go: picked in the confirm, else the default account.
  const [moveTo, setMoveTo] = createSignal<AccountId>();
  const [movingOff, setMovingOff] = createSignal<AccountId>();
  const others = (account: Account) =>
    props.profiles.accounts.filter((a) => a.id !== account.id);
  const target = (account: Account) => {
    const ids = others(account).map((a) => a.id);
    const picked = moveTo();
    const fallback = preselectedAccount(props.profiles);
    const id =
      picked !== undefined && ids.includes(picked)
        ? picked
        : fallback !== null && ids.includes(fallback)
          ? fallback
          : ids[0];
    return others(account).find((a) => a.id === id);
  };
  const moveAndRemove = (account: Account) => {
    const to = target(account);
    if (!props.onMoveTabs || !to) {
      remove(account, true);
      return;
    }
    setMovingOff(account.id);
    props
      .onMoveTabs(account.id, to)
      .then(() => {
        remove(account, true);
      })
      .catch((e: unknown) => {
        setRowError({ id: account.id, text: String(e) });
      })
      .finally(() => {
        setMovingOff(undefined);
      });
  };

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
          No accounts yet. Import one below or add a new one.
        </p>
      </Show>
      <ul class="accounts">
        <For each={props.profiles.accounts}>
          {(account) => (
            <li>
              <button
                type="button"
                class="dot-button"
                aria-label={`Colour: ${swatchOf(account.id, account.color).label}. Change ${account.name}'s colour`}
                aria-expanded={picking() === account.id}
                title="Change colour"
                onClick={() => {
                  setPicking((open) =>
                    open === account.id ? undefined : account.id,
                  );
                }}
              >
                <AccountDot id={account.id} color={account.color} />
              </button>
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
                    <Show when={twinOf(account)}>
                      {(twin) => (
                        <span class="account-twin">
                          Same login as {twin().name}
                        </span>
                      )}
                    </Show>
                    <span class="program">{account.configDir}</span>
                    <Show when={deletesOf(account)}>
                      {(deletes) => (
                        <span class="account-deletes">
                          {deletionText(deletes().days)}{" "}
                          <button
                            type="button"
                            onClick={() => {
                              keep(account);
                            }}
                          >
                            Keep them
                          </button>
                        </span>
                      )}
                    </Show>
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
                    title="Forgets the account. Its folder stays."
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
                    <Show
                      when={props.onMoveTabs && target(account)}
                      fallback={
                        <p class="hint">
                          Removing it stops them. The tabs stay, ready to move
                          to another account. Nothing is deleted.
                        </p>
                      }
                    >
                      {(to) => (
                        <>
                          <label class="move-to">
                            Move them to
                            <select
                              onChange={(event) => {
                                setMoveTo(Number(event.currentTarget.value));
                              }}
                            >
                              <For each={others(account)}>
                                {(other) => (
                                  <option
                                    value={other.id}
                                    selected={other.id === to().id}
                                  >
                                    {other.name}
                                  </option>
                                )}
                              </For>
                            </select>
                          </label>
                          <p class="hint">
                            Its folder stays. Nothing is deleted.
                          </p>
                          <Show
                            when={crossesOrganization(
                              logins(),
                              account.id,
                              to().id,
                            )}
                          >
                            <p class="hint">{OTHER_ORGANIZATION}</p>
                          </Show>
                        </>
                      )}
                    </Show>
                    <span class="row-confirm-actions">
                      <button
                        type="button"
                        class="danger"
                        disabled={movingOff() === account.id}
                        onClick={() => {
                          moveAndRemove(account);
                        }}
                      >
                        {props.onMoveTabs && target(account)
                          ? `Move and remove ${account.name}`
                          : `Remove ${account.name}`}
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
              <Show when={picking() === account.id}>
                <ColorPicker
                  id={account.id}
                  name={account.name}
                  color={account.color}
                  onPick={(color) => {
                    change(setAccountColor(account.id, color));
                  }}
                />
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
        Add opens Claude Code to sign in. Close the tab when done.
      </p>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
    </section>
  );
}
