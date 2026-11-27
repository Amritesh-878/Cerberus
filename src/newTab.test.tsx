import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { EMPTY } from "./harness";
import { NewTab } from "./NewTab";
import type { ProgramChoice } from "./pty";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

function programs(): ProgramChoice[] {
  const base = {
    args: [],
    imagePasteKey: null,
    installCommand: null,
    installShell: null,
    searched: null,
  };
  return [
    {
      ...base,
      id: "claude",
      label: "Claude Code",
      program: null,
      isShell: false,
    },
    {
      ...base,
      id: "fish",
      label: "fish",
      program: "/usr/bin/fish",
      isShell: true,
    },
    {
      ...base,
      id: "agy",
      label: "Antigravity CLI",
      program: "/usr/bin/agy",
      isShell: false,
    },
  ];
}

test("listing the programs again keeps focus on the same row", () => {
  const [listed, setListed] = createSignal(programs());
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(
    () => (
      <NewTab
        project={{ id: 1, name: "demo", folder: "/home/u/demo" }}
        planners={[]}
        profiles={EMPTY}
        programs={listed()}
        programsError={undefined}
        statuses={{ get: () => undefined, refresh: () => undefined }}
        anchor={undefined}
        side="beside"
        onLaunch={() => undefined}
        onDismiss={() => undefined}
      />
    ),
    root,
  );
  const focused = document.activeElement;
  expect(focused?.textContent).toContain("Antigravity CLI");

  setListed(programs());

  expect(document.activeElement).toBe(focused);
  expect(document.activeElement?.textContent).toContain("Antigravity CLI");
});
