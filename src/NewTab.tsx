import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import {
  type AccountId,
  preselectedAccount,
  type Profiles,
  statusText,
} from "./accounts";
import { ClaudeIcon, ShellIcon } from "./icons";
import type { ProgramChoice } from "./pty";
import { AccountDot } from "./Sidebar";
import type { Statuses } from "./statuses";
import { installed, shellChoice } from "./tab";
import type { Project, TabId, TreeTab } from "./tree";

const WIDTH = 280;
const MARGIN = 8;

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
  anchor: DOMRect | undefined;
  onLaunch: (launch: Launch) => void;
  onDismiss: () => void;
}) {
  const [accountId, setAccountId] = createSignal(
    untrack(() => preselectedAccount(props.profiles)),
  );
  const [parentId, setParentId] = createSignal<TabId | null>(null);
  const claude = () => installed(props.programs ?? [], "claude");
  const shell = () => shellChoice(props.profiles, props.programs ?? []);
  const position = () => {
    const anchor = props.anchor;
    if (!anchor) {
      return {
        left: `${String(Math.max(MARGIN, (innerWidth - WIDTH) / 2))}px`,
        top: "64px",
      };
    }
    const left = Math.max(
      MARGIN,
      Math.min(anchor.left, innerWidth - WIDTH - MARGIN),
    );
    return { left: `${String(left)}px`, top: `${String(anchor.bottom + 4)}px` };
  };
  const launch = (choice: ProgramChoice | undefined) => {
    if (choice)
      props.onLaunch({ choice, accountId: accountId(), parentId: parentId() });
  };

  let primary: HTMLButtonElement | undefined;
  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onDismiss();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
    });
    primary?.focus();
  });

  return (
    <div
      class="popover-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onDismiss();
      }}
    >
      <div
        class="popover new-tab"
        role="dialog"
        aria-label={`New tab in ${props.project.name}`}
        style={position()}
      >
        <p class="popover-where">
          New tab in <strong>{props.project.name}</strong>
        </p>
        <Show when={props.profiles.accounts.length > 0}>
          <p class="field-label">Account</p>
          <div class="choices">
            <For each={props.profiles.accounts}>
              {(account) => (
                <button
                  type="button"
                  aria-pressed={accountId() === account.id}
                  title={statusText(props.statuses.get(account.id))}
                  onClick={() => setAccountId(account.id)}
                >
                  <AccountDot id={account.id} />
                  {account.name}
                </button>
              )}
            </For>
          </div>
        </Show>
        <Show when={props.planners.length > 0}>
          <p class="field-label">Place</p>
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
        <button
          type="button"
          class="primary"
          disabled={!claude()}
          ref={(el) => {
            primary = el;
          }}
          onClick={() => {
            launch(claude());
          }}
        >
          <ClaudeIcon size={16} /> Claude Code
        </button>
        <button
          type="button"
          class="secondary"
          disabled={!shell()}
          onClick={() => {
            launch(shell());
          }}
        >
          <ShellIcon /> Shell
        </button>
        <Show when={props.programs && !claude()}>
          <p class="hint">Claude Code isn't installed.</p>
        </Show>
        <Show when={props.programsError !== undefined}>
          <p class="error">
            Could not list programs: {String(props.programsError)}
          </p>
        </Show>
      </div>
    </div>
  );
}
