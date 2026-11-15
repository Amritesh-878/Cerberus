import { For, onCleanup, onMount } from "solid-js";

export interface MenuItem {
  label: string;
  title?: string;
  pick: () => void;
}

/** A row's ⋯ menu: it closes on a pick, Escape or a press outside it other than on its own ⋯. */
export function RowMenu(props: { items: MenuItem[]; onClose: () => void }) {
  let menu: HTMLDivElement | undefined;
  onMount(() => {
    const onDown = (event: PointerEvent) => {
      // The row's own ⋯ toggles the menu on its click, so its press is not an outside click.
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
  return (
    <div
      class="pr-menu"
      role="menu"
      ref={(el) => {
        menu = el;
      }}
    >
      <For each={props.items}>
        {(item) => (
          <button
            type="button"
            role="menuitem"
            title={item.title}
            onClick={() => {
              props.onClose();
              item.pick();
            }}
          >
            {item.label}
          </button>
        )}
      </For>
    </div>
  );
}
