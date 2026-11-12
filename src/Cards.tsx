import { createSignal, type JSX, Show, untrack } from "solid-js";

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

/** Pointer handlers that let a card's header drag it to another place in the panel. */
function headerDrag(id: PanelCard, state: PanelState) {
  let start: { pointer: number; y: number } | undefined;
  let moved = false;
  return {
    onPointerDown: (event: PointerEvent & { currentTarget: HTMLElement }) => {
      // A button in the header other than the title does its own job.
      const button = (event.target as Element).closest("button");
      if (button && !button.classList.contains("card-title")) return;
      if (event.button !== 0 || event.pointerType === "touch") return;
      start = { pointer: event.pointerId, y: event.clientY };
      moved = false;
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    onPointerMove: (event: PointerEvent & { currentTarget: HTMLElement }) => {
      if (event.pointerId !== start?.pointer) return;
      if (!moved && Math.abs(event.clientY - start.y) < CARD_DRAG_PX) return;
      moved = true;
      setCardDrag({
        card: id,
        before: cardBefore(event.currentTarget, event.clientY),
      });
    },
    onPointerUp: (event: PointerEvent & { currentTarget: HTMLElement }) => {
      if (event.pointerId !== start?.pointer) return;
      start = undefined;
      const drop = cardDrag();
      setCardDrag(undefined);
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      if (moved && drop && drop.before !== drop.card) {
        state.moveCard(drop.card, drop.before ?? undefined);
      }
    },
    onPointerCancel: () => {
      start = undefined;
      setCardDrag(undefined);
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
        onPointerMove={drag.onPointerMove}
        onPointerUp={drag.onPointerUp}
        onPointerCancel={drag.onPointerCancel}
      >
        <Show
          when={props.fixed !== true}
          fallback={<span class="card-title">{props.title}</span>}
        >
          <button
            type="button"
            class="card-title"
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
