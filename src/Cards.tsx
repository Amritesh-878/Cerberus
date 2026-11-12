import { type JSX, Show } from "solid-js";

import type { PanelCard } from "./accounts";
import { ChevronIcon } from "./icons";
import type { PanelState } from "./panelPrefs";

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
  return (
    <section
      class="card"
      classList={{ folded: !open(), fixed: props.fixed === true }}
    >
      <h2 class="card-head">
        <Show
          when={props.fixed !== true}
          fallback={<span class="card-title">{props.title}</span>}
        >
          <button
            type="button"
            class="card-title"
            aria-expanded={open()}
            onClick={() => {
              props.state.toggleCard(props.id);
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
