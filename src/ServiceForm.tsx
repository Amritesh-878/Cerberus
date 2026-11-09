import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import { pickFolder } from "./accounts";
import { CloseIcon, FolderIcon } from "./icons";
import type { ProgramChoice } from "./pty";
import {
  BLANK,
  type Draft,
  devServerDraft,
  portOf,
  type ServiceRow,
  serviceCommand,
  tunnelDraft,
} from "./services";
import { keepInside } from "./ShortcutsSheet";
import { folderName } from "./tree";

/** Adds a service, or edits one; nothing is saved until Save. */
export function ServiceForm(props: {
  /** The service being edited; absent adds a new one. */
  editing?: ServiceRow;
  /** Installed shells, the Shell setting's first. */
  shells: ProgramChoice[];
  onSave: (draft: Draft) => Promise<void>;
  onClose: () => void;
}) {
  const start = untrack(() => props.editing);
  const [draft, setDraft] = createSignal<Draft>(
    start
      ? {
          name: start.name,
          command: "",
          folder: start.folder,
          shell: start.shell,
          startWithApp: start.startWithApp,
          restart: start.restart,
        }
      : BLANK,
  );
  const [asking, setAsking] = createSignal(false);
  const [port, setPort] = createSignal("5173");
  const [error, setError] = createSignal<string>();
  const [saving, setSaving] = createSignal(false);
  const [loaded, setLoaded] = createSignal(start === undefined);
  const set = (change: Partial<Draft>) => {
    setDraft((now) => ({ ...now, ...change }));
  };

  // Asked for only while editing, so the command is in the webview no longer than the form is open.
  if (start) {
    serviceCommand(start.id).then(
      (command) => {
        set({ command });
        setLoaded(true);
      },
      (failure: unknown) => setError(String(failure)),
    );
  }

  const close = () => {
    if (!saving()) props.onClose();
  };
  let sheet: HTMLDivElement | undefined;
  onMount(() => {
    const opener = document.activeElement;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
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

  const useTunnel = () => {
    const chosen = portOf(port());
    if (chosen === undefined) {
      setError("The port is a number from 1 to 65535");
      return;
    }
    setError(undefined);
    setDraft((now) => tunnelDraft(chosen, now));
    setAsking(false);
  };
  const pick = () => {
    pickFolder(draft().folder ?? undefined).then(
      (folder) => {
        if (folder !== null) set({ folder });
      },
      (failure: unknown) =>
        setError(`Could not open the picker: ${String(failure)}`),
    );
  };
  const save = () => {
    setSaving(true);
    props
      .onSave(draft())
      .then(props.onClose, (failure: unknown) => setError(String(failure)))
      .finally(() => setSaving(false));
  };

  return (
    <div
      class="sheet-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        class="sheet env-dialog service-form"
        role="dialog"
        aria-modal="true"
        aria-labelledby="service-title"
        tabIndex={-1}
        ref={(el) => {
          sheet = el;
        }}
      >
        <div class="sheet-head">
          <h2 id="service-title">
            {start ? `Edit ${start.name}` : "New service"}
          </h2>
          <button
            type="button"
            class="icon-button"
            aria-label="Close"
            onClick={close}
          >
            <CloseIcon />
          </button>
        </div>
        <Show when={!start}>
          <div class="service-presets">
            <span>Start from</span>
            <button
              type="button"
              class="secondary small"
              aria-expanded={asking()}
              onClick={() => setAsking((now) => !now)}
            >
              cloudflared tunnel
            </button>
            <button
              type="button"
              class="secondary small"
              onClick={() => {
                setAsking(false);
                setDraft((now) => devServerDraft(now));
              }}
            >
              Dev server
            </button>
          </div>
          <Show when={asking()}>
            <div class="service-port">
              <label>
                <span>Local port</span>
                <input
                  inputMode="numeric"
                  value={port()}
                  onInput={(event) => setPort(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") useTunnel();
                  }}
                />
              </label>
              <button type="button" class="secondary small" onClick={useTunnel}>
                Fill in
              </button>
            </div>
          </Show>
        </Show>
        <label class="env-field">
          <span>Name</span>
          <input
            value={draft().name}
            onInput={(event) => {
              set({ name: event.currentTarget.value });
            }}
          />
        </label>
        <label class="env-field">
          <span>Command</span>
          <input
            class="service-command"
            spellcheck={false}
            disabled={!loaded()}
            placeholder={loaded() ? "npm run dev" : "Reading…"}
            value={draft().command}
            onInput={(event) => {
              set({ command: event.currentTarget.value });
            }}
          />
        </label>
        <div class="env-row">
          <FolderIcon />
          <span class="env-name" title={draft().folder ?? "Your home folder"}>
            {draft().folder === null
              ? "Home folder"
              : folderName(draft().folder ?? "")}
          </span>
          <span class="spacer" />
          <button type="button" class="secondary small" onClick={pick}>
            Choose folder…
          </button>
        </div>
        <label class="env-field">
          <span>Shell</span>
          <select
            value={draft().shell ?? ""}
            onChange={(event) => {
              const value = event.currentTarget.value;
              set({ shell: value === "" ? null : value });
            }}
          >
            <option value="">The Shell setting's</option>
            <For each={props.shells}>
              {(shell) => <option value={shell.id}>{shell.label}</option>}
            </For>
          </select>
        </label>
        <label class="service-check">
          <input
            type="checkbox"
            checked={draft().startWithApp}
            onChange={(event) => {
              set({ startWithApp: event.currentTarget.checked });
            }}
          />
          <span>Start with Cerberus</span>
        </label>
        <label class="service-check">
          <input
            type="checkbox"
            checked={draft().restart}
            onChange={(event) => {
              set({ restart: event.currentTarget.checked });
            }}
          />
          <span>Restart if it stops</span>
        </label>
        <p class="env-note">
          It runs hidden in its own terminal and stops when Cerberus closes. The
          command is kept in Cerberus's data folder.
        </p>
        <Show when={error()}>
          {(message) => (
            <p class="error" role="alert">
              {message()}
            </p>
          )}
        </Show>
        <div class="env-actions">
          <button type="button" class="secondary" onClick={close}>
            Cancel
          </button>
          <button type="button" disabled={saving() || !loaded()} onClick={save}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
