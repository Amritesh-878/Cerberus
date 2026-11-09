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
  state: PanelState;
  children: JSX.Element;
}) {
  const open = () => !props.state.collapsed(props.id);
  return (
    <section class="card" classList={{ folded: !open() }}>
      <h2 class="card-head">
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
        <Show when={props.meta}>
          <span class="card-meta">{props.meta}</span>
        </Show>
        {props.action}
      </h2>
      <Show when={open()}>
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
