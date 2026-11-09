import { createSignal, For, onCleanup, Show } from "solid-js";

import { Card, Note, Row } from "./Cards";
import { CodeIcon } from "./icons";
import { elapsedText, openOutput, type TabLive } from "./live";
import type { PanelState } from "./panelPrefs";

/** Milliseconds since the epoch, ticking each second so a running task's time moves. */
function createSecondClock() {
  const [now, setNow] = createSignal(Date.now());
  const timer = setInterval(() => setNow(Date.now()), 1000);
  onCleanup(() => {
    clearInterval(timer);
  });
  return now;
}

/** The files the shown chat wrote, newest first; a click opens one in the editor. */
export function OutputsCard(props: {
  live: TabLive | undefined;
  liveOn: boolean;
  state: PanelState;
}) {
  const [failed, setFailed] = createSignal<string>();
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
                <button
                  type="button"
                  class="card-row action"
                  title={`${output.path}\nOpen in your editor`}
                  onClick={() => {
                    openOutput(live().tab, output.path).then(
                      () => {
                        setFailed(undefined);
                      },
                      (error: unknown) => {
                        setFailed(`Could not open it: ${String(error)}`);
                      },
                    );
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
}) {
  const now = createSecondClock();
  const running = () => props.live?.tasks ?? [];
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
          {(live) => (
            <>
              <For each={live().tasks} fallback={<Note>None running</Note>}>
                {(task) => (
                  <Row
                    icon={<span class="mark working" />}
                    text={task.label}
                    detail={task.kind === "subagent" ? "subagent" : null}
                    meta={elapsedText(now() - task.since)}
                    title={`Running for ${elapsedText(now() - task.since)}, counted from when Cerberus first saw it`}
                  />
                )}
              </For>
              <Show when={live().finished > 0}>
                <p class="card-note faint">{live().finished} finished</p>
              </Show>
            </>
          )}
        </Show>
      </Show>
    </Card>
  );
}
