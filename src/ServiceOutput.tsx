import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { onCleanup, onMount } from "solid-js";

import { terminalFont } from "./font";
import { PLATFORM } from "./keys";
import { type ServiceId, watchOutput } from "./services";
import { createRenderer } from "./Terminal";
import { throttle } from "./throttle";

const FIT_INTERVAL_MS = 30;

/** A service's output, read only: what it printed so far, then as it comes. */
export function ServiceOutput(props: { service: ServiceId }) {
  let host: HTMLDivElement | undefined;

  onMount(() => {
    if (!host) return;
    const element = host;
    const term = new Terminal({
      fontFamily: terminalFont(PLATFORM),
      fontSize: 14,
      cursorBlink: false,
      disableStdin: true,
      scrollback: 10_000,
      theme: { background: "#1a1a1a", cursor: "#1a1a1a" },
      allowProposedApi: true,
    });
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(element);
    const renderer = createRenderer(term);
    renderer.show();
    fit.fit();
    const refit = throttle(() => {
      fit.fit();
    }, FIT_INTERVAL_MS);
    const observer = new ResizeObserver(refit.call);
    observer.observe(element);
    // Read once: a new service gets a new view, keyed by the caller.
    const stop = watchOutput(props.service, (bytes) => {
      term.write(bytes);
    });
    const copy = term.onSelectionChange(() => {
      const text = term.getSelection();
      if (text) {
        navigator.clipboard.writeText(text).catch((error: unknown) => {
          console.error("copy failed:", error);
        });
      }
    });

    onCleanup(() => {
      stop();
      copy.dispose();
      observer.disconnect();
      refit.cancel();
      renderer.hide();
      term.dispose();
    });
  });

  return (
    <div
      class="terminal service-terminal"
      ref={(el) => {
        host = el;
      }}
    />
  );
}
