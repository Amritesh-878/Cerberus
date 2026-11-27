import {
  createMemo,
  createSignal,
  For,
  Index,
  Match,
  onCleanup,
  onMount,
  Show,
  Switch,
  untrack,
} from "solid-js";

import { backdropDismiss } from "./backdrop";
import {
  type AccountId,
  preselectedAccount,
  type Profiles,
  statusText,
} from "./accounts";
import { AccountDot, ClaudeIcon, ProgramIcon, ShellIcon } from "./icons";
import { type Anchor, placeBox, type Side } from "./place";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { installed, OTHER_CLIS, shellChoice } from "./tab";
import type { Project, TabId, TreeTab } from "./tree";

const WIDTH = 280;

interface Entry {
  kind: "claude" | "shell" | "cli";
  choice: ProgramChoice | undefined;
}

export interface Launch {
  choice: ProgramChoice;
  accountId: AccountId | null;
  parentId: TabId | null;
}

export function NewTab(props: {
  project: Project;
  planners: TreeTab[];
  profiles: Profiles;
  programs: ProgramChoice[] | undefined;
  programsError: unknown;
  statuses: Statuses;
  anchor: Anchor | undefined;
  side: Side;
  onLaunch: (launch: Launch) => void;
  onDismiss: () => void;
}) {
  const [accountId, setAccountId] = createSignal(
    untrack(() => preselectedAccount(props.profiles)),
  );
  const [parentId, setParentId] = createSignal<TabId | null>(null);
  const claude = () => installed(props.programs ?? [], "claude");
  const claudeChoice = () => props.programs?.find((p) => p.id === "claude");
  const installShellLabel = () => {
    const id = claudeChoice()?.installShell ?? null;
    const named = props.programs?.find((p) => p.id === id)?.label;
    return named ?? "a shell";
  };
  const shell = () => shellChoice(props.profiles, props.programs ?? []);
  // Listed only when installed; the guide says how to install them.
  const otherClis = () =>
    OTHER_CLIS.flatMap((id) => installed(props.programs ?? [], id) ?? []);
  // Without Claude Code, Antigravity CLI then Codex CLI come first, so Enter opens one of them.
  const entries = createMemo((): Entry[] => {
    const shellEntry: Entry = { kind: "shell", choice: shell() };
    const clis = otherClis().map((choice): Entry => ({ kind: "cli", choice }));
    const found = claude();
    return found
      ? [{ kind: "claude", choice: found }, shellEntry, ...clis]
      : [...clis, shellEntry];
  });
  const primary = () => entries().find((entry) => entry.choice !== undefined);
  const [height, setHeight] = createSignal(0);
  const [viewport, setViewport] = createSignal({
    width: innerWidth,
    height: innerHeight,
  });
  const position = () => {
    const place = placeBox(
      props.anchor,
      { width: WIDTH, height: height() },
      viewport(),
      props.side,
    );
    return { left: `${String(place.left)}px`, top: `${String(place.top)}px` };
  };
  const launch = (choice: ProgramChoice | undefined) => {
    if (choice)
      props.onLaunch({ choice, accountId: accountId(), parentId: parentId() });
  };

  let box: HTMLDivElement | undefined;
  onMount(() => {
    const onResize = () => {
      setViewport({ width: innerWidth, height: innerHeight });
    };
    window.addEventListener("resize", onResize);
    // Measured once drawn, so a box near the bottom moves up to stay in the window.
    const observer = new ResizeObserver(() => {
      setHeight(box?.offsetHeight ?? 0);
    });
    if (box) observer.observe(box);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onDismiss();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      observer.disconnect();
      window.removeEventListener("resize", onResize);
    });
    (
      box?.querySelector<HTMLButtonElement>(".menu-item:not(:disabled)") ??
      box?.querySelector("button")
    )?.focus();
  });

  return (
    <div
      class="popover-backdrop"
      {...backdropDismiss(() => {
        props.onDismiss();
      })}
    >
      <div
        class="popover menu new-tab"
        ref={(el) => {
          box = el;
        }}
        role="dialog"
        aria-label={`New tab in ${props.project.name}`}
        style={position()}
      >
        <p class="menu-title">
          New tab in <strong>{props.project.name}</strong>
        </p>
        <Show when={props.profiles.accounts.length > 0}>
          <p class="menu-label">Account</p>
          <div class="choices">
            <For each={props.profiles.accounts}>
              {(account) => (
                <button
                  type="button"
                  aria-pressed={accountId() === account.id}
                  title={statusText(props.statuses.get(account.id))}
                  onClick={() => setAccountId(account.id)}
                >
                  <AccountDot id={account.id} color={account.color} />
                  {account.name}
                </button>
              )}
            </For>
          </div>
        </Show>
        <Show when={props.planners.length > 0}>
          <p class="menu-label">Place</p>
          <div class="choices">
            <button
              type="button"
              aria-pressed={parentId() === null}
              onClick={() => setParentId(null)}
            >
              On its own
            </button>
            <For each={props.planners}>
              {(planner) => (
                <button
                  type="button"
                  aria-pressed={parentId() === planner.id}
                  onClick={() => setParentId(planner.id)}
                >
                  Under {planner.title}
                </button>
              )}
            </For>
          </div>
        </Show>
        <div class="menu-sep" role="separator" />
        {/* Rows keep their buttons when the programs are listed again, so the focused one keeps focus. */}
        <Index each={entries()}>
          {(entry) => (
            <button
              type="button"
              class="menu-item"
              aria-label={entry().kind === "claude" ? "Claude Code" : undefined}
              disabled={!entry().choice}
              onClick={() => {
                launch(entry().choice);
              }}
            >
              <Switch>
                <Match when={entry().kind === "claude"}>
                  <ClaudeIcon size={14} />
                </Match>
                <Match when={entry().kind === "shell"}>
                  <ShellIcon />
                </Match>
                <Match when={entry().choice}>
                  {(choice) => <ProgramIcon program={choice().id} />}
                </Match>
              </Switch>
              <span class="menu-text">
                {entry().kind === "shell" ? "Shell" : entry().choice?.label}
              </span>
              <Show when={entry() === primary()}>
                <span class="menu-key">Enter</span>
              </Show>
            </button>
          )}
        </Index>
        <Show when={props.programs && !claude()}>
          <p class="menu-note-line">
            Claude Code isn't installed. {claudeChoice()?.searched}
          </p>
          <Show when={claudeChoice()?.installCommand}>
            {(command) => (
              <p class="menu-note-line">
                To install it, run <code class="menu-command">{command()}</code>{" "}
                in {installShellLabel()}.
              </p>
            )}
          </Show>
        </Show>
        <Show when={props.programsError !== undefined}>
          <p class="menu-note-line error">
            Could not list programs: {String(props.programsError)}
          </p>
        </Show>
      </div>
    </div>
  );
}
