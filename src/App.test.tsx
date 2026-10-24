import { render } from "solid-js/web";
import { afterEach, expect, test } from "vitest";

import { App } from "./App";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

test("renders the app heading", () => {
  const root = document.createElement("div");
  document.body.append(root);
  dispose = render(() => <App />, root);

  expect(root.querySelector("h1")?.textContent).toBe("Cerberus");
});
