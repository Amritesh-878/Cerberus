import { createSignal, type JSX, onCleanup, Show, untrack } from "solid-js";

import type { PanelCard } from "./accounts";
import { ChevronIcon } from "./icons";
import type { PanelState } from "./panelPrefs";

// Past this many pixels a press on a card's header moves the card, not toggles it.
const CARD_DRAG_PX = 6;

/** The card being dragged by its header, and the card it would land in front of (`null` for the end). */
const [cardDrag, setCardDrag] = createSignal<{
  card: PanelCard;
  before: PanelCard | null;
}>();

/** The card whose upper half is below `y`, among the panel's cards on screen; none means the end. */
function cardBefore(head: HTMLElement, y: number): PanelCard | null {
  const cards = [
    ...(head
      .closest(".panel-cards")
      ?.querySelectorAll<HTMLElement>(":scope > .card") ?? []),
  ].sort(
    (a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top,
  );
  const found = cards.find((card) => {
    const box = card.getBoundingClientRect();
    return y < box.top + box.height / 2;
  });
  return (found?.dataset.card as PanelCard | undefined) ?? null;
}

const FOLLOWED = ["pointermove", "pointerup", "pointercancel"] as const;

/** Pointer handlers that let a card's header drag it to another place in the panel. */
function headerDrag(id: PanelCard, state: PanelState) {
  let start: { pointer: number; y: number; head: HTMLElement } | undefined;
  let moved = false;
  const stop = () => {
    for (const type of FOLLOWED) window.removeEventListener(type, follow, true);
    start = undefined;
    // Every card stops on unmount; only the card being dragged may drop the drag.
    if (untrack(cardDrag)?.card === id) setCardDrag(undefined);
  };
  // Followed on the window, never by pointer capture: WebView2 sends the click after a captured press to the header, not the title.
  function follow(event: PointerEvent) {
    if (event.pointerId !== start?.pointer) return;
    // A move with no button held means the release happened outside the window, so the drag ends and the card stays.
    if (
      event.type === "pointercancel" ||
      (event.type === "pointermove" && event.buttons === 0)
    ) {
      stop();
      // No click follows a drag that ended this way, so the title's next one (Enter or Space) is a real one.
      moved = false;
      return;
    }
    if (event.type === "pointerup") {
      const drop = cardDrag();
      stop();
      if (moved && drop && drop.before !== drop.card) {
        state.moveCard(drop.card, drop.before ?? undefined);
      }
      // Only the click this release makes is the drag's own; one released off the title makes none.
      setTimeout(() => {
        moved = false;
      }, 0);
      return;
    }
    if (!moved && Math.abs(event.clientY - start.y) < CARD_DRAG_PX) return;
    moved = true;
    setCardDrag({ card: id, before: cardBefore(start.head, event.clientY) });
  }
  onCleanup(stop);
  return {
    onPointerDown: (event: PointerEvent & { currentTarget: HTMLElement }) => {
      // A button in the header other than the title does its own job.
      const button = (event.target as Element).closest("button");
      if (button && !button.classList.contains("card-title")) return;
      if (event.button !== 0 || event.pointerType === "touch") return;
      stop();
      start = {
        pointer: event.pointerId,
        y: event.clientY,
        head: event.currentTarget,
      };
      moved = false;
      for (const type of FOLLOWED) window.addEventListener(type, follow, true);
    },
    /** True once after a drag, for the click on the title that ends it. */
    swallowClick: () => {
      const was = moved;
      moved = false;
      return was;
    },
  };
}

export function Card(props: {
  id: PanelCard;
  title: string;
  meta?: string;
  /** One line under the title while the card is open. */
  subtitle?: string;
  /** What the card shows, as the title's hover hint. */
  hint?: string;
  /** A button at the right of the title, such as Edit environment. */
  action?: JSX.Element;
  /** Shown in the title row between the title and the meta, such as a meter. */
  inline?: JSX.Element;
  /** The whole card fits its title row, so it never folds; `note` shows under it when there is something to say. */
  fixed?: boolean;
  note?: string;
  state: PanelState;
  children?: JSX.Element;
}) {
  const open = () => props.fixed === true || !props.state.collapsed(props.id);
  // A card's id and the panel's state never change for it, so they are read once.
  const drag = headerDrag(
    untrack(() => props.id),
    untrack(() => props.state),
  );
  return (
    <section
      class="card"
      data-card={props.id}
      style={{ order: props.state.orderOf(props.id) }}
      classList={{
        folded: !open(),
        fixed: props.fixed === true,
        dragging: cardDrag()?.card === props.id,
        "drop-before":
          cardDrag() !== undefined &&
          cardDrag()?.before === props.id &&
          cardDrag()?.card !== props.id,
      }}
    >
      <h2
        class="card-head"
        title="Drag to move the card"
        onPointerDown={drag.onPointerDown}
      >
        <Show
          when={props.fixed !== true}
          fallback={
            <span class="card-title" title={props.hint}>
              {props.title}
            </span>
          }
        >
          <button
            type="button"
            class="card-title"
            title={props.hint}
            aria-expanded={open()}
            onClick={() => {
              if (!drag.swallowClick()) props.state.toggleCard(props.id);
            }}
          >
            {props.title}
            <ChevronIcon />
          </button>
        </Show>
        {props.inline}
        <Show when={props.meta}>
          <span class="card-meta">{props.meta}</span>
        </Show>
        {props.action}
      </h2>
      <Show when={props.fixed === true ? props.note : undefined}>
        {(note) => (
          <div class="card-body">
            <Note>{note()}</Note>
          </div>
        )}
      </Show>
      <Show when={open() && props.fixed !== true}>
        <div class="card-body">
          <Show when={props.subtitle}>
            <p class="card-subtitle">{props.subtitle}</p>
          </Show>
          {props.children}
        </div>
      </Show>
    </section>
  );
}

export function Row(props: {
  icon: JSX.Element;
  text: string;
  detail?: string | null;
  meta?: JSX.Element;
  title?: string;
}) {
  return (
    <div class="card-row" title={props.title}>
      <span class="row-icon">{props.icon}</span>
      <span class="row-text">
        <span class="row-main">{props.text}</span>
        <Show when={props.detail}>
          <span class="row-detail">{props.detail}</span>
        </Show>
      </span>
      <Show when={props.meta}>
        <span class="row-meta">{props.meta}</span>
      </Show>
    </div>
  );
}

export function Note(props: { children: JSX.Element }) {
  return <p class="card-note">{props.children}</p>;
}
