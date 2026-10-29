import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { createEffect, onCleanup, onMount } from "solid-js";

import { pasteInput, terminalHandlesKey } from "./keys";
import { PtySession, type SpawnSpec } from "./pty";
import { throttle } from "./throttle";

const FIT_INTERVAL_MS = 30;

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

export function TerminalView(props: {
  spec: SpawnSpec;
  pastesImages: boolean;
  input: string | null;
  active: boolean;
}) {
  let host: HTMLDivElement | undefined;

  onMount(() => {
    if (!host) return;
    const element = host;
    let disposed = false;
    const term = new Terminal({
      fontFamily: '"Cascadia Mono", Consolas, monospace',
      fontSize: 14,
      cursorBlink: true,
      scrollback: 10_000,
      theme: { background: "#1a1a1a" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(element);
    useWebgl(term);
    fit.fit();
    term.attachCustomKeyEventHandler(terminalHandlesKey);

    let input = props.input;
    const session = new PtySession(
      props.spec,
      { cols: term.cols, rows: term.rows },
      {
        output: (bytes) => {
          if (disposed) return;
          term.write(bytes, () => {
            session.drawn(bytes.length);
            // After the first draw, so it follows xterm's answer to ConPTY's cursor query.
            if (input) session.write(input);
            input = null;
          });
        },
        exit: (code) => {
          if (disposed) return;
          term.write(
            `\r\n\x1b[2m[exited with code ${code === null ? "unknown" : String(code)}]\x1b[0m\r\n`,
          );
        },
      },
    );
    session.started.catch((error: unknown) => {
      if (!disposed) term.write(`\x1b[31m${String(error)}\x1b[0m\r\n`);
    });

    const pastesImages = props.pastesImages;
    // Capture phase, so an image paste never reaches xterm's own text-paste handler.
    const onPaste = (event: ClipboardEvent) => {
      const input =
        event.clipboardData && pasteInput(event.clipboardData, pastesImages);
      if (!input) return;
      event.preventDefault();
      event.stopPropagation();
      session.write(input);
    };
    element.addEventListener("paste", onPaste, true);

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
    const refit = throttle(() => {
      fit.fit();
    }, FIT_INTERVAL_MS);
    const observer = new ResizeObserver(refit.call);
    observer.observe(element);
    createEffect(() => {
      if (props.active) term.focus();
    });

    onCleanup(() => {
      disposed = true;
      observer.disconnect();
      refit.cancel();
      element.removeEventListener("paste", onPaste, true);
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
