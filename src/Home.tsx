import { For, Show } from "solid-js";

import { AppIcon, WarnIcon } from "./icons";
import { keyHelp } from "./keys";
import { type Gap, GAP_TEXT } from "./steps";

/** The start page: what to do next, what setup still lacks, and the keys. */
export function Home(props: {
  /** The project the last shown tab is in, where the main button opens a new tab. */
  project: string | undefined;
  hasProjects: boolean;
  gaps: readonly Gap[];
  treeError: string | undefined;
  treeNotice: string | undefined;
  onNewTab: (anchor: DOMRect) => void;
  onNewProject: () => void;
  onSetup: (gap: Gap) => void;
}) {
  return (
    <main class="home">
      <header class="home-head">
        <AppIcon size={32} />
        <h1>Cerberus</h1>
      </header>
      <Show when={props.treeNotice}>
        {(notice) => <p class="hint">{notice()}</p>}
      </Show>
      <Show when={props.treeError}>
        {(error) => (
          <p class="error">
            {error()}. Tabs opened in this window will not be saved.
          </p>
        )}
      </Show>
      <Show when={props.gaps[0]}>
        {(first) => (
          <div class="home-setup" role="status">
            <p class="home-setup-title">Setup isn't finished</p>
            <ul>
              <For each={props.gaps}>
                {(gap) => (
                  <li>
                    <WarnIcon />
                    {GAP_TEXT[gap]}
                  </li>
                )}
              </For>
            </ul>
            <button
              type="button"
              class="secondary"
              onClick={() => {
                props.onSetup(first());
              }}
            >
              Finish setup
            </button>
          </div>
        )}
      </Show>
      <Show
        when={props.hasProjects && props.project}
        fallback={
          <>
            <p class="home-lead">Add a project</p>
            <p class="hint">
              A project is a folder. Every tab in it starts there.
            </p>
            <div class="home-actions">
              <button
                type="button"
                class="primary"
                onClick={() => {
                  props.onNewProject();
                }}
              >
                New project
              </button>
            </div>
          </>
        }
      >
        {(project) => (
          <div class="home-actions">
            <button
              type="button"
              class="primary"
              title={`In ${project()}`}
              onClick={(event) => {
                props.onNewTab(event.currentTarget.getBoundingClientRect());
              }}
            >
              New Claude Code tab
            </button>
            <button
              type="button"
              class="secondary"
              onClick={() => {
                props.onNewProject();
              }}
            >
              New project
            </button>
          </div>
        )}
      </Show>
      <p class="home-label">Keys</p>
      <dl class="home-keys">
        <For each={keyHelp()}>
          {(help) => (
            <>
              <dt>{help.does}</dt>
              <dd>
                <For each={help.keys.split(" / ")}>
                  {(combo) => <kbd>{combo}</kbd>}
                </For>
              </dd>
            </>
          )}
        </For>
      </dl>
    </main>
  );
}
