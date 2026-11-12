import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { backdropDismiss } from "./backdrop";
import { Card, Note, Row } from "./Cards";
import { CodeIcon, MoreIcon } from "./icons";
import { elapsedText, openOutput, type Output, type TabLive } from "./live";
import type { PanelState } from "./panelPrefs";
import { placeBox } from "./place";
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

/** A written file's menu, for what the pane can't do. */
function OutputMenu(props: {
  output: Output;
  at: DOMRect;
  onVsCode: () => void;
  onDismiss: () => void;
}) {
  const [place, setPlace] = createSignal({ left: 0, top: 0 });
  let box: HTMLDivElement | undefined;
  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onDismiss();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
    });
    const size = {
      width: box?.offsetWidth ?? 0,
      height: box?.offsetHeight ?? 0,
    };
    setPlace(
      placeBox(props.at, size, { width: innerWidth, height: innerHeight }),
    );
    box?.querySelector<HTMLButtonElement>("button")?.focus();
  });
  return (
    <div
      class="popover-backdrop"
      {...backdropDismiss(() => {
        props.onDismiss();
      })}
    >
      <div
        class="popover menu service-menu"
        role="menu"
        aria-label={`${props.output.name} menu`}
        style={{
          left: `${String(place().left)}px`,
          top: `${String(place().top)}px`,
        }}
        ref={(el) => {
          box = el;
        }}
      >
        <button
          type="button"
          class="menu-item"
          role="menuitem"
          onClick={() => {
            props.onVsCode();
          }}
        >
          <span class="menu-text">Open in VS Code</span>
        </button>
      </div>
    </div>
  );
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
  const [menu, setMenu] = createSignal<{ output: Output; at: DOMRect }>();
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
                    aria-label={`More for ${output.name}`}
                    aria-haspopup="menu"
                    onClick={(event) => {
                      setMenu({
                        output,
                        at: event.currentTarget.getBoundingClientRect(),
                      });
                    }}
                  >
                    <MoreIcon />
                  </button>
                </div>
              )}
            </For>
          )}
        </Show>
        <Show when={failed()}>{(message) => <Note>{message()}</Note>}</Show>
        <Show when={menu()}>
          {(open) => (
            <OutputMenu
              output={open().output}
              at={open().at}
              onVsCode={() => {
                const path = open().output.path;
                const tab = props.live?.tab;
                setMenu(undefined);
                if (tab !== undefined) vsCode(tab, path);
              }}
              onDismiss={() => {
                setMenu(undefined);
              }}
            />
          )}
        </Show>
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
