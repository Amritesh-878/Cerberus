import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { onCleanup, onMount } from "solid-js";

import { terminalHandlesKey } from "./keys";
import { PtySession, type SpawnSpec } from "./pty";

const FIT_DELAY_MS = 50;

function useWebgl(term: Terminal): void {
  try {
    const webgl = new WebglAddon();
    // Disposing the addon on a lost context hands drawing back to the DOM renderer.
    webgl.onContextLoss(() => {
      webgl.dispose();
    });
    term.loadAddon(webgl);
  } catch (error) {
    console.warn("WebGL unavailable, using the DOM renderer:", error);
  }
}

function copySelection(term: Terminal): void {
  const text = term.getSelection();
  if (!text) return;
  navigator.clipboard.writeText(text).catch((error: unknown) => {
    console.error("copy failed:", error);
  });
}

export function TerminalView(props: { spec: SpawnSpec }) {
  let host: HTMLDivElement | undefined;

  onMount(() => {
    if (!host) return;
    const term = new Terminal({
      fontFamily: '"Cascadia Mono", Consolas, monospace',
      fontSize: 14,
      cursorBlink: true,
      scrollback: 10_000,
      theme: { background: "#1a1a1a" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    useWebgl(term);
    fit.fit();
    term.attachCustomKeyEventHandler(terminalHandlesKey);

    const session = new PtySession(
      props.spec,
      { cols: term.cols, rows: term.rows },
      {
        output: (bytes) => {
          term.write(bytes);
        },
        exit: (code) => {
          term.write(
            `\r\n\x1b[2m[exited with code ${code === null ? "unknown" : String(code)}]\x1b[0m\r\n`,
          );
        },
      },
    );
    session.started.catch((error: unknown) => {
      term.write(`\x1b[31m${String(error)}\x1b[0m\r\n`);
    });

    const subscriptions = [
      term.onData((data) => {
        session.write(data);
      }),
      term.onResize((size) => {
        session.resize(size);
      }),
      term.onSelectionChange(() => {
        copySelection(term);
      }),
    ];
    let fitTimer: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(() => {
      clearTimeout(fitTimer);
      fitTimer = setTimeout(() => {
        fit.fit();
      }, FIT_DELAY_MS);
    });
    observer.observe(host);
    term.focus();

    onCleanup(() => {
      observer.disconnect();
      clearTimeout(fitTimer);
      subscriptions.forEach((subscription) => {
        subscription.dispose();
      });
      session.kill();
      term.dispose();
    });
  });

  return (
    <div
      class="terminal"
      ref={(el) => {
        host = el;
      }}
    />
  );
}
