import { createSignal, For, onCleanup, Show } from "solid-js";

import { Card, Note } from "./Cards";
import { CodeIcon } from "./icons";
import {
  elapsedText,
  openOutput,
  type RunningTask,
  type TabLive,
} from "./live";
import type { PanelState } from "./panelPrefs";
import type { TabId } from "./tree";

/** Milliseconds since the epoch, ticking each second so a running task's time moves. */
function createSecondClock() {
  const [now, setNow] = createSignal(Date.now());
  const timer = setInterval(() => setNow(Date.now()), 1000);
  onCleanup(() => {
    clearInterval(timer);
  });
  return now;
}

/** The files the shown chat wrote, newest first; a click shows one in the file pane. */
export function OutputsCard(props: {
  live: TabLive | undefined;
  liveOn: boolean;
  state: PanelState;
  /** The file the pane shows, marked in the list. */
  opened?: string;
  onOpenFile: (tab: TabId, path: string) => void;
}) {
  const [failed, setFailed] = createSignal<string>();
  const vsCode = (tab: TabId, path: string) => {
    openOutput(tab, path).then(
      () => {
        setFailed(undefined);
      },
      (error: unknown) => {
        setFailed(`Could not open it: ${String(error)}`);
      },
    );
  };
  const outputs = () => props.live?.outputs ?? [];
  return (
    <Card
      id="outputs"
      title="Outputs"
      meta={outputs().length > 0 ? String(outputs().length) : undefined}
      state={props.state}
    >
      <Show
        when={props.liveOn}
        fallback={<Note>Live cards are off in Accounts</Note>}
      >
        <Show
          when={props.live}
          fallback={<Note>Restart the chat to see its files here</Note>}
        >
          {(live) => (
            <For
              each={live().outputs}
              fallback={<Note>No files written yet</Note>}
            >
              {(output) => (
                <div
                  class="card-row output-row"
                  classList={{ opened: props.opened === output.path }}
                >
                  <button
                    type="button"
                    class="output-open"
                    title={output.path}
                    aria-current={
                      props.opened === output.path ? "true" : undefined
                    }
                    onClick={() => {
                      props.onOpenFile(live().tab, output.path);
                    }}
                  >
                    <span class="row-icon">
                      <CodeIcon />
                    </span>
                    <span class="row-text">
                      <span class="row-main">{output.name}</span>
                      <Show when={output.folder}>
                        <span class="row-detail">{output.folder}</span>
                      </Show>
                    </span>
                  </button>
                  <button
                    type="button"
                    class="row-button icon"
                    aria-label={`Open ${output.name} in VS Code`}
                    title="Open in VS Code"
                    onClick={() => {
                      vsCode(live().tab, output.path);
                    }}
                  >
                    <CodeIcon />
                  </button>
                </div>
              )}
            </For>
          )}
        </Show>
        <Show when={failed()}>{(message) => <Note>{message()}</Note>}</Show>
      </Show>
    </Card>
  );
}

/** The shown chat's background shells and subagents, with how long each has run. */
export function TasksCard(props: {
  live: TabLive | undefined;
  liveOn: boolean;
  state: PanelState;
  /** The shown chat can be typed into, so a row can offer Claude Code's own `/tasks` view. */
  canType: boolean;
  onType: (text: string) => void;
}) {
  const now = createSecondClock();
  const running = () => props.live?.tasks ?? [];
  // Running first, then finished, each newest first.
  const rows = () => [
    ...running().slice().reverse(),
    ...(props.live?.done ?? []),
  ];
  const ranFor = (task: RunningTask) =>
    elapsedText((task.ended ?? now()) - task.since);
  return (
    <Card
      id="tasks"
      title="Background tasks"
      meta={
        running().length > 0 ? `${String(running().length)} running` : undefined
      }
      state={props.state}
    >
      <Show
        when={props.liveOn}
        fallback={<Note>Live cards are off in Accounts</Note>}
      >
        <Show
          when={props.live}
          fallback={<Note>Restart the chat to see its tasks here</Note>}
        >
          <For each={rows()} fallback={<Note>None yet</Note>}>
            {(task) => (
              <button
                type="button"
                class="card-row task-row"
                classList={{ done: task.ended !== null }}
                disabled={!props.canType}
                title={
                  props.canType
                    ? "Types /tasks into the chat; press Enter there to see its output"
                    : "Open the running chat to see its tasks"
                }
                onClick={() => {
                  props.onType("/tasks");
                }}
              >
                <span class="row-icon">
                  <span
                    class="mark"
                    classList={{
                      working: task.ended === null,
                      idle: task.ended !== null,
                    }}
                  />
                </span>
                <span class="row-text">
                  <span class="row-main">{task.description ?? task.label}</span>
                  <span class="row-detail">
                    {task.kind === "subagent" && task.label !== "subagent"
                      ? `subagent · ${task.label}`
                      : task.label}
                    {task.ended === null ? " · running" : " · done"}
                  </span>
                </span>
                <span class="row-meta">{ranFor(task)}</span>
              </button>
            )}
          </For>
          <Show
            when={(props.live?.finished ?? 0) > (props.live?.done.length ?? 0)}
          >
            <p class="card-note faint">
              {props.live?.finished} finished in all
            </p>
          </Show>
        </Show>
      </Show>
    </Card>
  );
}
