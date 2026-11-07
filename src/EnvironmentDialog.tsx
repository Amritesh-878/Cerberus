import {
  createResource,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  untrack,
} from "solid-js";

import { pickFolder } from "./accounts";
import {
  type EnvEdit,
  type EnvironmentChange,
  planFolders,
  tabEnv,
} from "./environment";
import { BranchIcon, CloseIcon, FolderIcon } from "./icons";
import { panelChanges } from "./panelData";
import { keepInside } from "./ShortcutsSheet";
import { folderName, type TreeTab } from "./tree";

/** The folder's branch, or nothing for a folder outside git or one that cannot be read. */
function FolderBranch(props: { folder: string }) {
  const [changes] = createResource(
    () => props.folder,
    (folder) => panelChanges(folder).catch(() => undefined),
  );
  const branch = () => {
    const found = changes();
    if (found?.kind !== "repo") return undefined;
    return found.branch ?? `detached at ${found.commit ?? "HEAD"}`;
  };
  return (
    <Show when={branch()}>
      {(name) => (
        <span class="env-branch">
          <BranchIcon />
          {name()}
        </span>
      )}
    </Show>
  );
}

/** Edits the shown tab's name, folders and variables; nothing changes until Save. */
export function EnvironmentDialog(props: {
  tab: TreeTab;
  /** A Claude Code chat runs in the tab, so an added folder is typed in as `/add-dir`. */
  chatRunning: boolean;
  /** The chat is working, so moving it asks first. */
  busy: boolean;
  onSave: (change: EnvironmentChange) => Promise<void>;
  onClose: () => void;
}) {
  // The tab as it was on open, which the form edits and the plan compares against.
  const spec = untrack(() => props.tab.launch.spec);
  const [title, setTitle] = createSignal(untrack(() => props.tab.title));
  const [cwd, setCwd] = createSignal(spec.cwd);
  const [dirs, setDirs] = createSignal<string[]>([...(spec.dirs ?? [])]);
  const [saved] = createResource(() => props.tab.id, tabEnv);
  const [removedVars, setRemovedVars] = createSignal<string[]>([]);
  const [addedVars, setAddedVars] = createSignal<
    { name: string; value: string }[]
  >([]);
  const [varName, setVarName] = createSignal("");
  const [varValue, setVarValue] = createSignal("");
  const [error, setError] = createSignal<string>();
  const [confirming, setConfirming] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const plan = () =>
    planFolders(
      { cwd: spec.cwd, dirs: spec.dirs ?? [] },
      { cwd: cwd(), dirs: dirs() },
    );
  const varNames = () => [
    ...(saved() ?? []).filter((n) => !removedVars().includes(n)),
    ...addedVars()
      .map((v) => v.name)
      .filter((n) => !(saved() ?? []).includes(n)),
  ];

  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    const opener = document.activeElement;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
      else if (event.key === "Tab" && sheet && keepInside(sheet, event)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    });
    sheet?.querySelector<HTMLInputElement>("input")?.focus();
  });

  const pick = (then: (folder: string) => void) => {
    pickFolder(cwd() ?? undefined).then(
      (folder) => {
        if (folder !== null) then(folder);
      },
      (failure: unknown) =>
        setError(`Could not open the picker: ${String(failure)}`),
    );
  };
  const addVar = () => {
    const name = varName().trim();
    if (name === "") return;
    setAddedVars((all) => [
      ...all.filter((v) => v.name !== name),
      { name, value: varValue() },
    ]);
    setRemovedVars((all) => all.filter((n) => n !== name));
    setVarName("");
    setVarValue("");
  };
  const removeVar = (name: string) => {
    setAddedVars((all) => all.filter((v) => v.name !== name));
    if ((saved() ?? []).includes(name)) {
      setRemovedVars((all) => [...all, name]);
    }
  };
  const save = () => {
    if (plan().restart && props.busy && !confirming()) {
      setConfirming(true);
      return;
    }
    const env: EnvEdit[] = [
      ...removedVars().map((name) => ({ name, value: null })),
      ...addedVars(),
    ];
    setSaving(true);
    props
      .onSave({ title: title(), cwd: cwd(), dirs: dirs(), env })
      .then(props.onClose, (failure: unknown) => {
        setConfirming(false);
        setError(String(failure));
      })
      .finally(() => setSaving(false));
  };

  return (
    <div
      class="sheet-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        class="sheet env-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="env-title"
        tabIndex={-1}
        ref={(el) => {
          sheet = el;
        }}
      >
        <div class="sheet-head">
          <h2 id="env-title">Edit environment</h2>
          <button
            type="button"
            class="icon-button"
            aria-label="Close"
            onClick={() => {
              props.onClose();
            }}
          >
            <CloseIcon />
          </button>
        </div>
        <label class="env-field">
          <span>Name</span>
          <input
            value={title()}
            onInput={(event) => setTitle(event.currentTarget.value)}
          />
        </label>
        <section class="sheet-section">
          <h3>Folders</h3>
          <div class="env-row">
            <FolderIcon />
            <span class="env-name" title={cwd() ?? "Home folder"}>
              {cwd() === null ? "Home folder" : folderName(cwd() ?? "")}
            </span>
            <span class="env-tag">main</span>
            <Show when={cwd()}>
              {(folder) => <FolderBranch folder={folder()} />}
            </Show>
            <span class="spacer" />
            <button
              type="button"
              class="secondary small"
              onClick={() => {
                pick((folder) => {
                  setCwd(folder);
                  setDirs((all) => all.filter((d) => d !== folder));
                });
              }}
            >
              Change…
            </button>
          </div>
          <For each={dirs()}>
            {(dir) => (
              <div class="env-row">
                <FolderIcon />
                <span class="env-name" title={dir}>
                  {folderName(dir)}
                </span>
                <FolderBranch folder={dir} />
                <span class="spacer" />
                <button
                  type="button"
                  class="icon-button"
                  aria-label={`Remove ${folderName(dir)}`}
                  onClick={() => setDirs((all) => all.filter((d) => d !== dir))}
                >
                  <CloseIcon />
                </button>
              </div>
            )}
          </For>
          <button
            type="button"
            class="secondary small"
            onClick={() => {
              pick((folder) => {
                if (folder !== cwd() && !dirs().includes(folder)) {
                  setDirs((all) => [...all, folder]);
                }
              });
            }}
          >
            Add folder…
          </button>
          <Show when={plan().restart}>
            <p class="env-note">
              The tab restarts in the new folder and resumes the same chat.
            </p>
          </Show>
          <Show when={plan().added.length > 0 && props.chatRunning}>
            <p class="env-note">
              {plan().added.length === 1
                ? "/add-dir is typed into the chat for you to send."
                : "/add-dir is typed into the chat for the first folder; the rest apply on the next start."}
            </p>
          </Show>
          <Show when={plan().removed.length > 0}>
            <p class="env-note">Removing a folder applies on the next start.</p>
          </Show>
        </section>
        <section class="sheet-section">
          <h3>Environment variables</h3>
          <For each={varNames()}>
            {(name) => (
              <div class="env-row">
                <code class="env-name">{name}</code>
                <span class="env-hidden">value hidden</span>
                <span class="spacer" />
                <button
                  type="button"
                  class="icon-button"
                  aria-label={`Remove ${name}`}
                  onClick={() => {
                    removeVar(name);
                  }}
                >
                  <CloseIcon />
                </button>
              </div>
            )}
          </For>
          <div class="env-add">
            <input
              placeholder="NAME"
              aria-label="Variable name"
              spellcheck={false}
              value={varName()}
              onInput={(event) => setVarName(event.currentTarget.value)}
            />
            <input
              type="password"
              placeholder="value"
              aria-label="Variable value"
              autocomplete="off"
              value={varValue()}
              onInput={(event) => setVarValue(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") addVar();
              }}
            />
            <button type="button" class="secondary small" onClick={addVar}>
              Add
            </button>
          </div>
          <p class="env-note">
            Set when the tab starts. Stored in plain text in Cerberus's tabs
            file, so keep keys in your own secret store.
          </p>
        </section>
        <Show when={error()}>
          {(message) => (
            <p class="error" role="alert">
              {message()}
            </p>
          )}
        </Show>
        <div class="env-actions">
          <Show
            when={confirming()}
            fallback={
              <>
                <button
                  type="button"
                  class="secondary"
                  onClick={() => {
                    props.onClose();
                  }}
                >
                  Cancel
                </button>
                <button type="button" disabled={saving()} onClick={save}>
                  Save
                </button>
              </>
            }
          >
            <span class="env-confirm" role="alert">
              The chat is working. Restart it in the new folder now?
            </span>
            <button
              type="button"
              class="secondary"
              onClick={() => setConfirming(false)}
            >
              Back
            </button>
            <button type="button" disabled={saving()} onClick={save}>
              Restart
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
