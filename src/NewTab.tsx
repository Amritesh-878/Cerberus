import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import {
  type AccountId,
  preselectedAccount,
  type Profiles,
  statusText,
} from "./accounts";
import { AccountDot, ClaudeIcon, ShellIcon } from "./icons";
import { type Anchor, placeBox, type Side } from "./place";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { installed, shellChoice } from "./tab";
import type { Project, TabId, TreeTab } from "./tree";

const WIDTH = 280;

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
  const shell = () => shellChoice(props.profiles, props.programs ?? []);
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

  let primary: HTMLButtonElement | undefined;
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
    (primary?.disabled ? box?.querySelector("button") : primary)?.focus();
  });

  return (
    <div
      class="popover-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onDismiss();
      }}
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
        <button
          type="button"
          class="menu-item"
          aria-label="Claude Code"
          disabled={!claude()}
          ref={(el) => {
            primary = el;
          }}
          onClick={() => {
            launch(claude());
          }}
        >
          <ClaudeIcon size={14} />
          <span class="menu-text">Claude Code</span>
          <span class="menu-key">Enter</span>
        </button>
        <button
          type="button"
          class="menu-item"
          disabled={!shell()}
          onClick={() => {
            launch(shell());
          }}
        >
          <ShellIcon />
          <span class="menu-text">Shell</span>
        </button>
        <Show when={props.programs && !claude()}>
          <p class="menu-note-line">Claude Code isn't installed.</p>
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
