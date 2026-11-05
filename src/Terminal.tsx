import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";

import type { LiveState } from "./board";
import { terminalFont } from "./font";
import {
  PLATFORM,
  pasteInput,
  type TerminalAction,
  terminalAction,
  terminalHandlesKey,
} from "./keys";
import { PtySession, type SpawnSpec } from "./pty";
import { throttle } from "./throttle";

const FIT_INTERVAL_MS = 30;
// Long enough for a large chat to load; the first state report usually ends it sooner.
const RESUMING_NOTICE_MS = 20_000;
const MISSING_NOTICE_MS = 10_000;
const RESUMING = "Resuming chat…";
const MISSING = "The saved chat is gone, so this is a new one.";
const ENDED = "The chat ended. Resume chat is in the tab's menu.";
const ENDED_LINE =
  "\r\n\x1b[2m[the chat ended; this tab is now a shell]\x1b[0m\r\n";

function canvasesOf(term: Terminal): HTMLCanvasElement[] {
  return [...(term.element?.querySelectorAll("canvas") ?? [])];
}

// WebKit keeps only a few WebGL contexts alive, so only the tab on screen holds one.
function createRenderer(term: Terminal): {
  show: () => void;
  hide: () => void;
} {
  let webgl: WebglAddon | undefined;
  let contexts: WebGL2RenderingContext[] = [];
  const hide = () => {
    webgl?.dispose();
    webgl = undefined;
    // The addon leaves its context to the garbage collector, which may not run before the next tab needs one.
    for (const gl of contexts) {
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    }
    contexts = [];
  };
  const show = () => {
    if (webgl) return;
    try {
      const addon = new WebglAddon();
      // Disposing the addon on a lost context hands drawing back to the DOM renderer.
      addon.onContextLoss(() => {
        if (webgl === addon) hide();
      });
      const before = new Set(canvasesOf(term));
      term.loadAddon(addon);
      webgl = addon;
      // A canvas the addon drew in 2D has no WebGL context, so it is skipped.
      contexts = canvasesOf(term)
        .filter((canvas) => !before.has(canvas))
        .flatMap((canvas) => canvas.getContext("webgl2") ?? []);
    } catch (error) {
      console.warn("WebGL unavailable, using the DOM renderer:", error);
    }
  };
  return { show, hide };
}

function run(term: Terminal, action: TerminalAction): void {
  if (action.kind === "send") term.input(action.bytes);
  else if (action.kind === "clear") term.clear();
  else term.selectAll();
}

function copySelection(term: Terminal): void {
  const text = term.getSelection();
  if (!text) return;
  navigator.clipboard.writeText(text).catch((error: unknown) => {
    console.error("copy failed:", error);
  });
}

export function TerminalView(props: {
  tab: number;
  spec: SpawnSpec;
  imagePasteKey: string | null;
  input: string | null;
  active: boolean;
  onState: (state: LiveState) => void;
  onExit: () => void;
  /** The chat ended by itself, so this pane now runs the shell. */
  onEnded: () => void;
  /** Hands over a function that types into this pane without Enter; the returned function takes it back. */
  onTyper?: (type: (text: string) => void) => () => void;
}) {
  let host: HTMLDivElement | undefined;
  const [notice, setNotice] = createSignal<string>();

  onMount(() => {
    if (!host) return;
    const element = host;
    let disposed = false;
    const term = new Terminal({
      fontFamily: terminalFont(PLATFORM),
      fontSize: 14,
      cursorBlink: true,
      scrollback: 10_000,
      theme: { background: "#1a1a1a" },
      // Option+B, Option+F and Option+arrows move by word in shells, as in Terminal.app with this set.
      macOptionIsMeta: PLATFORM === "mac",
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(element);
    const renderer = createRenderer(term);
    fit.fit();
    term.attachCustomKeyEventHandler((event) => {
      const action =
        event.type === "keydown" ? terminalAction(event) : undefined;
      if (!action) return terminalHandlesKey(event);
      event.preventDefault();
      run(term, action);
      return false;
    });

    // Read once: a pane keeps its tab and program spec for its whole life.
    const spec = props.spec;
    const tab = props.tab;
    let imagePasteKey = props.imagePasteKey;
    let noticeTimer: ReturnType<typeof setTimeout> | undefined;
    const showNotice = (text: string, ms: number) => {
      clearTimeout(noticeTimer);
      setNotice(text);
      noticeTimer = setTimeout(() => {
        if (notice() === text) setNotice(undefined);
      }, ms);
    };
    // Starts the tab's program in this pane; after an ended chat the core runs the shell instead.
    const start = (firstInput: string | null): PtySession => {
      let input = firstInput;
      const current = new PtySession(
        spec,
        { cols: term.cols, rows: term.rows },
        {
          output: (bytes) => {
            if (disposed) return;
            term.write(bytes, () => {
              current.drawn(bytes.length);
              // After the first draw, so it follows xterm's answer to ConPTY's cursor query.
              if (input) current.write(input);
              input = null;
            });
          },
          exit: (code, ended) => {
            if (disposed) return;
            if (ended) {
              term.write(ENDED_LINE);
              props.onEnded();
              session = start(null);
              return;
            }
            term.write(
              `\r\n\x1b[2m[exited with code ${code === null ? "unknown" : String(code)}]\x1b[0m\r\n`,
            );
            props.onExit();
          },
          state: (state) => {
            if (disposed) return;
            if (notice() === RESUMING) setNotice(undefined);
            props.onState(state);
          },
        },
        tab,
      );
      current.started.catch((error: unknown) => {
        if (!disposed) term.write(`\x1b[31m${String(error)}\x1b[0m\r\n`);
      });
      void current.resume.then((resume) => {
        if (disposed || resume === "new") return;
        if (resume === "ended") {
          imagePasteKey = null;
          props.onEnded();
          showNotice(ENDED, MISSING_NOTICE_MS);
        } else if (resume === "resuming") {
          showNotice(RESUMING, RESUMING_NOTICE_MS);
        } else {
          showNotice(MISSING, MISSING_NOTICE_MS);
        }
      });
      return current;
    };
    let session = start(props.input);

    // Capture phase, so an image paste never reaches xterm's own text-paste handler.
    const onPaste = (event: ClipboardEvent) => {
      const input =
        event.clipboardData && pasteInput(event.clipboardData, imagePasteKey);
      if (!input) return;
      event.preventDefault();
      event.stopPropagation();
      session.write(input);
    };
    element.addEventListener("paste", onPaste, true);
    const untype = props.onTyper?.((text) => {
      session.write(text);
      term.focus();
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
    const refit = throttle(() => {
      fit.fit();
    }, FIT_INTERVAL_MS);
    const observer = new ResizeObserver(refit.call);
    observer.observe(element);
    createEffect(() => {
      if (!props.active) {
        renderer.hide();
        return;
      }
      renderer.show();
      term.focus();
    });

    onCleanup(() => {
      disposed = true;
      untype?.();
      clearTimeout(noticeTimer);
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
    <>
      <div
        class="terminal"
        ref={(el) => {
          host = el;
        }}
      />
      <Show when={notice()}>
        {(text) => (
          <button
            type="button"
            class="terminal-notice"
            title="Dismiss"
            onClick={() => {
              setNotice(undefined);
            }}
          >
            {text()}
          </button>
        )}
      </Show>
    </>
  );
}
