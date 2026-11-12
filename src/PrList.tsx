import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { ExternalIcon, MoreIcon, PrIcon } from "./icons";
import type { PrRow } from "./panelData";
import type { RepoPrs } from "./panelFeed";

/** Rows shown at first and added by each Show more. */
export const PR_PAGE = 5;

const STATE_TEXT: Record<PrRow["state"], string> = {
  draft: "Draft",
  open: "Open",
  merged: "Merged",
  closed: "Closed",
};

export interface PrActions {
  open: (folder: string, pr: PrRow) => void;
  copy: (folder: string, pr: PrRow) => void;
  dismiss: (pr: PrRow) => void;
  /** Called only after the user confirmed in the menu. */
  ready: (folder: string, pr: PrRow) => void;
}

function PrMenu(props: {
  pr: PrRow;
  folder: string;
  actions: PrActions;
  onClose: () => void;
}) {
  const [confirming, setConfirming] = createSignal(false);
  let menu: HTMLDivElement | undefined;
  onMount(() => {
    const onDown = (event: PointerEvent) => {
      // This row's own ⋯ toggles the menu on its click, so its press is not an outside click.
      const trigger = menu?.parentElement?.querySelector(
        '[aria-haspopup="menu"]',
      );
      if (
        menu &&
        event.target instanceof Node &&
        !menu.contains(event.target) &&
        !trigger?.contains(event.target)
      ) {
        props.onClose();
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
    };
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    });
    menu?.querySelector("button")?.focus();
  });
  const pick = (action: () => void) => {
    props.onClose();
    action();
  };
  return (
    <div
      class="pr-menu"
      role="menu"
      ref={(el) => {
        menu = el;
      }}
    >
      <Show
        when={confirming()}
        fallback={
          <>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                pick(() => {
                  props.actions.copy(props.folder, props.pr);
                });
              }}
            >
              Copy link
            </button>
            <Show when={props.pr.state === "draft"}>
              <button
                type="button"
                role="menuitem"
                onClick={() => setConfirming(true)}
              >
                Ready for review…
              </button>
            </Show>
            <button
              type="button"
              role="menuitem"
              title="Hidden until the PR changes"
              onClick={() => {
                pick(() => {
                  props.actions.dismiss(props.pr);
                });
              }}
            >
              Dismiss
            </button>
          </>
        }
      >
        <p class="pr-confirm">
          Mark #{props.pr.number} ready for review on GitHub?
        </p>
        <button
          type="button"
          role="menuitem"
          onClick={() => {
            pick(() => {
              props.actions.ready(props.folder, props.pr);
            });
          }}
        >
          Mark ready
        </button>
        <button
          type="button"
          role="menuitem"
          onClick={() => {
            props.onClose();
          }}
        >
          Cancel
        </button>
      </Show>
    </div>
  );
}

/** A repo's PRs, newest first, five at a time; dismissed ones stay hidden until they change. */
export function PrList(props: {
  repo: RepoPrs;
  isDismissed: (pr: PrRow) => boolean;
  actions: PrActions;
}) {
  const [shown, setShown] = createSignal(PR_PAGE);
  const [menuFor, setMenuFor] = createSignal<number>();
  const visible = () => props.repo.prs.filter((pr) => !props.isDismissed(pr));
  // Keyed by number, so a poll's fresh objects keep each row, and an open menu or confirm, in place.
  const numbers = () =>
    visible()
      .slice(0, shown())
      .map((pr) => pr.number);
  return (
    <>
      <For each={numbers()}>
        {(number) => (
          <Show when={props.repo.prs.find((pr) => pr.number === number)}>
            {(pr) => (
              <div class={`card-row pr-row ${pr().state}`}>
                <span class="row-icon pr">
                  <PrIcon />
                </span>
                <span class="row-text">
                  <span class="row-main" title={pr().title}>
                    {pr().title}
                  </span>
                  <span class="row-detail">
                    {props.repo.name} #{number}
                  </span>
                </span>
                <span class={`pr-state ${pr().state}`}>
                  {STATE_TEXT[pr().state]}
                </span>
                <button
                  type="button"
                  class="row-button icon"
                  aria-label={`Open #${String(number)} on GitHub`}
                  title="Open on GitHub"
                  onClick={() => {
                    props.actions.open(props.repo.folder, pr());
                  }}
                >
                  <ExternalIcon />
                </button>
                <button
                  type="button"
                  class="row-button icon"
                  aria-label={`More for #${String(number)}`}
                  aria-haspopup="menu"
                  aria-expanded={menuFor() === number}
                  onClick={() =>
                    setMenuFor((open) => (open === number ? undefined : number))
                  }
                >
                  <MoreIcon />
                </button>
                <Show when={menuFor() === number}>
                  <PrMenu
                    pr={pr()}
                    folder={props.repo.folder}
                    actions={props.actions}
                    onClose={() => {
                      setMenuFor(undefined);
                    }}
                  />
                </Show>
              </div>
            )}
          </Show>
        )}
      </For>
      <Show when={visible().length > shown()}>
        <button
          type="button"
          class="show-more"
          onClick={() => setShown((n) => n + PR_PAGE)}
        >
          Show more
        </button>
      </Show>
    </>
  );
}
