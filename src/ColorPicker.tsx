import { For, onMount } from "solid-js";

import { type AccountId, PALETTE, swatchOf } from "./accounts";
import { CheckIcon } from "./icons";

const STEPS: Readonly<Record<string, number>> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/** The swatch a key moves to from `at`, wrapping as a radio group does; `undefined` for other keys. */
export function swatchAfter(at: number, key: string): number | undefined {
  const count = PALETTE.length;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  const step = STEPS[key];
  return step === undefined ? undefined : (at + step + count) % count;
}

/** The palette as radio buttons: the chosen one has a ring and a check, so colour is never the only sign. */
export function ColorPicker(props: {
  id: AccountId;
  name: string;
  color: string | undefined;
  onPick: (color: string) => void;
}) {
  const chosen = () => swatchOf(props.id, props.color).name;
  let group: HTMLDivElement | undefined;
  // Each save brings new account objects and a new row, so the chosen swatch takes focus back.
  onMount(() => {
    group?.querySelector<HTMLButtonElement>("[aria-checked=true]")?.focus();
  });

  return (
    <div
      class="swatches"
      role="radiogroup"
      aria-label={`Colour for ${props.name}`}
      ref={(el) => {
        group = el;
      }}
      onKeyDown={(event) => {
        const at = PALETTE.findIndex((s) => s.name === chosen());
        const next = swatchAfter(at, event.key);
        const swatch = next === undefined ? undefined : PALETTE[next];
        if (!swatch) return;
        event.preventDefault();
        props.onPick(swatch.name);
        group
          ?.querySelectorAll<HTMLButtonElement>("[role=radio]")
          [next ?? 0]?.focus();
      }}
    >
      <For each={PALETTE}>
        {(swatch) => (
          <button
            type="button"
            role="radio"
            class="swatch"
            aria-checked={swatch.name === chosen()}
            aria-label={swatch.label}
            title={swatch.label}
            tabIndex={swatch.name === chosen() ? 0 : -1}
            style={{ "--swatch": swatch.hex }}
            onClick={() => {
              props.onPick(swatch.name);
            }}
          >
            <CheckIcon />
          </button>
        )}
      </For>
    </div>
  );
}
