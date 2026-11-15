import { open as openFile } from "@tauri-apps/plugin-dialog";
import {
  createEffect,
  createResource,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
  untrack,
} from "solid-js";

import { type AccountId, pickFolder, type Profiles } from "./accounts";
import { backdropDismiss } from "./backdrop";
import { AccountDot, CloseIcon } from "./icons";
import { keepInside } from "./ShortcutsSheet";
import {
  defaultPrompt,
  guessTaskFolder,
  type TaskChoice,
  taskAccount,
  taskBranches,
  taskFiles,
  worktreeDefaults,
} from "./startTask";
import {
  folderName,
  type Project,
  type ProjectTasks,
  type TreeTab,
} from "./tree";

const PLACEHOLDERS =
  "{task_file} {task_id} {folder} {branch} {handoff_file} {planner_tab}";

/** Picks a task file, account and folder for a coder tab; nothing opens until Open. */
export function TaskDialog(props: {
  project: Project;
  /** The tab it was opened from, whose folders hold the task folder and whose account is the default. */
  tab: TreeTab;
  profiles: Profiles;
  /** Opens the coder tab; a rejection keeps the dialog open with its message. */
  onOpen: (choice: TaskChoice) => Promise<void>;
  /** Remembers the project's task folder, prompt or account as they are chosen. */
  onTasks: (change: Partial<ProjectTasks>) => void;
  onClose: () => void;
}) {
  const tasks = untrack(() => props.project.tasks);
  const tabFolders = untrack(() => [
    ...(props.tab.launch.spec.cwd ? [props.tab.launch.spec.cwd] : []),
    ...(props.tab.launch.spec.dirs ?? []),
    props.project.folder,
  ]);
  const [folder, setFolder] = createSignal<string | null | undefined>(
    tasks?.folder ?? undefined,
  );
  onMount(() => {
    if (folder() !== undefined) return;
    // A folder chosen while the guess was out stays chosen.
    guessTaskFolder(tabFolders).then(
      (guess) => {
        if (folder() === undefined) setFolder(guess);
      },
      () => {
        if (folder() === undefined) setFolder(null);
      },
    );
  });
  // A folder that cannot be read says so, rather than looking empty.
  const [listing] = createResource(
    () => folder() ?? undefined,
    (dir) =>
      taskFiles(dir).then(
        (found) => ({ files: found, error: undefined }),
        (failure: unknown) => ({ files: [], error: String(failure) }),
      ),
  );
  const files = () => listing()?.files;
  const [picked, setPicked] = createSignal<string>();
  const taskFile = () => picked() ?? files()?.[0]?.path;
  const [account, setAccount] = createSignal<AccountId | null>(
    untrack(() => taskAccount(tasks, props.tab, props.profiles)),
  );

  const [inWorktree, setInWorktree] = createSignal(false);
  const [branches] = createResource(
    () => (inWorktree() ? props.project.folder : undefined),
    (repo) => taskBranches(repo),
  );
  const [base, setBase] = createSignal("");
  const [wtFolder, setWtFolder] = createSignal("");
  const [wtBranch, setWtBranch] = createSignal("");
  createEffect(() => {
    const found = branches();
    if (found && base() === "") setBase(found.default ?? found.names[0] ?? "");
  });
  // A new task file names a new worktree, unless the names were already typed over.
  const [typed, setTyped] = createSignal(false);
  // The task file the shown names were made for, so a late answer for another file is dropped.
  const [namedFor, setNamedFor] = createSignal<string>();
  createEffect(
    on([taskFile, inWorktree], ([file, wanted]) => {
      if (!file || !wanted || untrack(typed)) return;
      worktreeDefaults(props.project.folder, file).then(
        (d) => {
          if (untrack(typed) || untrack(taskFile) !== file) return;
          setWtFolder(d.folder);
          setWtBranch(d.branch);
          setNamedFor(file);
        },
        () => undefined,
      );
    }),
  );
  // Open waits until the names belong to the task picked, unless they were typed by hand.
  const namesPending = () =>
    inWorktree() && !typed() && namedFor() !== taskFile();

  const [standard] = createResource(defaultPrompt);
  const [prompt, setPrompt] = createSignal<string | null>(
    tasks?.prompt ?? null,
  );
  const shownPrompt = () => prompt() ?? standard() ?? "";

  const [opening, setOpening] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const close = () => {
    if (!opening()) props.onClose();
  };

  const chooseFolder = () => {
    pickFolder(folder() ?? props.project.folder).then(
      (dir) => {
        if (dir === null) return;
        setFolder(dir);
        setPicked(undefined);
        props.onTasks({ folder: dir });
      },
      (failure: unknown) =>
        setError(`Could not open the picker: ${String(failure)}`),
    );
  };
  const chooseFile = () => {
    openFile({
      defaultPath: folder() ?? props.project.folder,
      filters: [{ name: "Task", extensions: ["md"] }],
    }).then(
      (file) => {
        if (typeof file === "string") setPicked(file);
      },
      (failure: unknown) =>
        setError(`Could not open the picker: ${String(failure)}`),
    );
  };

  const openTask = () => {
    const file = taskFile();
    if (!file) return;
    const template =
      prompt() === null || prompt() === standard() ? null : prompt();
    const worktree = inWorktree()
      ? { folder: wtFolder().trim(), branch: wtBranch().trim(), base: base() }
      : null;
    if (worktree && (!worktree.folder || !worktree.branch || !worktree.base)) {
      setError("A new worktree needs a folder, a branch and a base branch.");
      return;
    }
    setOpening(true);
    setError(undefined);
    props
      .onOpen({ taskFile: file, accountId: account(), worktree, template })
      .then(
        () => {
          props.onTasks({ account: account(), prompt: template });
          props.onClose();
        },
        (failure: unknown) => setError(String(failure)),
      )
      .finally(() => setOpening(false));
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
    sheet?.focus();
  });

  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

  return (
    <div class="sheet-backdrop" {...backdropDismiss(close)}>
      <div
        class="sheet env-dialog task-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="task-title"
        tabIndex={-1}
        ref={(el) => {
          sheet = el;
        }}
      >
        <div class="sheet-head">
          <h2 id="task-title">Start a task</h2>
          <button
            type="button"
            class="icon-button"
            aria-label="Close"
            onClick={close}
          >
            <CloseIcon />
          </button>
        </div>

        <section class="sheet-section">
          <div class="env-row">
            <h3>Task file</h3>
            <span class="spacer" />
            <button
              type="button"
              class="secondary small"
              onClick={chooseFolder}
            >
              {folder() ? "Change folder…" : "Choose folder…"}
            </button>
            <button type="button" class="secondary small" onClick={chooseFile}>
              Choose file…
            </button>
          </div>
          <Show when={folder()}>
            {(dir) => (
              <p class="env-note" title={dir()}>
                In {folderName(dir())}
              </p>
            )}
          </Show>
          <Show when={folder() === null && !picked()}>
            <p class="env-note">
              No tasks folder found. Choose one, or a single file.
            </p>
          </Show>
          <div class="task-files" role="radiogroup" aria-label="Task file">
            <Show when={picked() && !files()?.some((f) => f.path === picked())}>
              <label class="task-file">
                <input
                  type="radio"
                  name="task-file"
                  checked
                  onChange={() => undefined}
                />
                <span class="task-id">{fileName(picked() ?? "")}</span>
              </label>
            </Show>
            <For
              each={files()}
              fallback={
                <Show when={folder() ? listing() : undefined}>
                  {(read) => (
                    <p class="env-note">
                      {read().error ?? "No Markdown files in this folder."}
                    </p>
                  )}
                </Show>
              }
            >
              {(file) => (
                <label class="task-file" title={file.path}>
                  <input
                    type="radio"
                    name="task-file"
                    checked={taskFile() === file.path}
                    onChange={() => setPicked(file.path)}
                  />
                  <span class="task-id">{file.id}</span>
                  <span class="task-heading">{file.heading ?? ""}</span>
                </label>
              )}
            </For>
          </div>
        </section>

        <Show when={props.profiles.accounts.length > 0}>
          <section class="sheet-section">
            <h3>Account</h3>
            <div class="choices">
              <For each={props.profiles.accounts}>
                {(a) => (
                  <button
                    type="button"
                    aria-pressed={account() === a.id}
                    onClick={() => setAccount(a.id)}
                  >
                    <AccountDot id={a.id} color={a.color} />
                    {a.name}
                  </button>
                )}
              </For>
            </div>
          </section>
        </Show>

        <section class="sheet-section">
          <h3>Folder</h3>
          <div class="choices">
            <button
              type="button"
              aria-pressed={!inWorktree()}
              onClick={() => setInWorktree(false)}
            >
              {folderName(props.project.folder)}
            </button>
            <button
              type="button"
              aria-pressed={inWorktree()}
              onClick={() => setInWorktree(true)}
            >
              New worktree
            </button>
          </div>
          <Show when={inWorktree()}>
            <label class="env-field">
              <span>Base branch</span>
              <select
                value={base()}
                onChange={(e) => setBase(e.currentTarget.value)}
              >
                <For each={branches()?.names ?? []}>
                  {(name) => <option value={name}>{name}</option>}
                </For>
              </select>
            </label>
            <label class="env-field">
              <span>Worktree folder</span>
              <input
                value={wtFolder()}
                onInput={(e) => {
                  setTyped(true);
                  setWtFolder(e.currentTarget.value);
                }}
              />
            </label>
            <label class="env-field">
              <span>New branch</span>
              <input
                value={wtBranch()}
                onInput={(e) => {
                  setTyped(true);
                  setWtBranch(e.currentTarget.value);
                }}
              />
            </label>
            <p class="env-note">
              The chat also gets {folderName(props.project.folder)} as an added
              folder.
            </p>
          </Show>
        </section>

        <details class="sheet-section task-prompt">
          <summary>Task prompt</summary>
          <textarea
            aria-label="Task prompt"
            rows={5}
            value={shownPrompt()}
            onInput={(e) => setPrompt(e.currentTarget.value)}
          />
          <div class="env-row">
            <p class="env-note">Placeholders: {PLACEHOLDERS}</p>
            <span class="spacer" />
            <button
              type="button"
              class="secondary small"
              disabled={prompt() === null}
              onClick={() => setPrompt(null)}
            >
              Reset to default
            </button>
          </div>
        </details>

        <p class="env-note">
          The prompt is typed into the new chat once it is ready, and sent only
          when you press Enter.
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
          <button
            type="button"
            disabled={opening() || !taskFile() || namesPending()}
            onClick={openTask}
          >
            {opening() ? "Opening…" : "Open"}
          </button>
        </div>
      </div>
    </div>
  );
}
