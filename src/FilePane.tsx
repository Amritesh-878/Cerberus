import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { unifiedMergeView } from "@codemirror/merge";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  highlightSpecialChars,
  lineNumbers,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import {
  createEffect,
  createSignal,
  on,
  onCleanup,
  onMount,
  Show,
} from "solid-js";

import { terminalFont } from "./font";
import { PLATFORM } from "./keys";
import {
  type FileView,
  openOutput,
  type Output,
  outputStamp,
  readOutput,
} from "./live";
import type { TabId } from "./tree";

/** How often the pane asks whether the file changed on disk. */
export const POLL_MS = 500;

const LANGUAGES: Record<string, () => Extension> = {
  ts: () => javascript({ typescript: true }),
  mts: () => javascript({ typescript: true }),
  cts: () => javascript({ typescript: true }),
  tsx: () => javascript({ typescript: true, jsx: true }),
  js: () => javascript(),
  mjs: () => javascript(),
  cjs: () => javascript(),
  jsx: () => javascript({ jsx: true }),
  rs: () => rust(),
  py: () => python(),
  json: () => json(),
  md: () => markdown(),
  markdown: () => markdown(),
  css: () => css(),
  html: () => html(),
  htm: () => html(),
};

/** The file's language by its extension; `undefined` shows it plain. */
export function languageOf(path: string): string | undefined {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return undefined;
  const extension = name.slice(dot + 1).toLowerCase();
  return extension in LANGUAGES ? extension : undefined;
}

/** The one replaced span between two texts, so a reload keeps every unchanged line where it was. */
export function changeBetween(
  before: string,
  after: string,
): { from: number; to: number; insert: string } | undefined {
  if (before === after) return undefined;
  const most = Math.min(before.length, after.length);
  let start = 0;
  while (start < most && before.charCodeAt(start) === after.charCodeAt(start)) {
    start++;
  }
  let end = 0;
  while (
    end < most - start &&
    before.charCodeAt(before.length - 1 - end) ===
      after.charCodeAt(after.length - 1 - end)
  ) {
    end++;
  }
  return {
    from: start,
    to: before.length - end,
    insert: after.slice(start, after.length - end),
  };
}

// The editor keeps lines split on \n, so a CRLF file compares as the same text.
function lines(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}

/** Why the pane shows no text, or `undefined` when it does. */
export function refusal(read: FileView): string | undefined {
  switch (read.kind) {
    case "text":
      return undefined;
    case "tooBig":
      return `This file is ${(read.size / (1024 * 1024)).toFixed(1)} MB, over the 2 MB the pane shows.`;
    case "binary":
      return "This file is binary, so the pane can't show it.";
    case "gone":
      return "This file is gone from disk.";
  }
}

const COLOURS = HighlightStyle.define([
  {
    tag: [
      tags.keyword,
      tags.modifier,
      tags.controlKeyword,
      tags.operatorKeyword,
      tags.definitionKeyword,
      tags.moduleKeyword,
      tags.tagName,
    ],
    color: "#d98b6f",
  },
  {
    tag: [tags.string, tags.special(tags.string), tags.regexp],
    color: "#9ccf8a",
  },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "#d6b06b" },
  { tag: tags.comment, color: "#6f6f6f", fontStyle: "italic" },
  { tag: [tags.typeName, tags.className, tags.namespace], color: "#7fb4d6" },
  {
    tag: [
      tags.function(tags.variableName),
      tags.function(tags.propertyName),
      tags.macroName,
    ],
    color: "#e0c68a",
  },
  { tag: [tags.propertyName, tags.attributeName], color: "#b8c4d0" },
  { tag: tags.heading, color: "#e6e6e6", fontWeight: "bold" },
  { tag: tags.link, color: "#7fb4d6", textDecoration: "underline" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: [tags.meta, tags.processingInstruction], color: "#8a8a8a" },
  { tag: tags.invalid, color: "#e5736b" },
]);

const THEME = EditorView.theme(
  {
    "&": {
      height: "100%",
      color: "var(--text)",
      backgroundColor: "var(--bg)",
      fontSize: "13px",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": {
      fontFamily: terminalFont(PLATFORM),
      lineHeight: "1.5",
    },
    ".cm-gutters": {
      color: "var(--faint)",
      backgroundColor: "var(--bg)",
      border: "none",
    },
    ".cm-cursor": { borderLeftColor: "var(--text)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "#3a3a3a",
    },
    ".cm-changedLine": { backgroundColor: "rgb(108 192 122 / 10%)" },
    ".cm-changedText": { backgroundColor: "rgb(108 192 122 / 25%)" },
    ".cm-deletedChunk": { backgroundColor: "rgb(229 115 107 / 10%)" },
    ".cm-deletedChunk .cm-deletedText": {
      backgroundColor: "rgb(229 115 107 / 25%)",
    },
    ".cm-insertedLine, .cm-deletedLine": { textDecoration: "none" },
  },
  { dark: true },
);

/** A file the chat wrote, read only, beside the shown tab; it reloads when the file changes on disk. */
export default function FilePane(props: {
  tab: TabId;
  path: string;
  /** The chat's files, newest first, so Follow can move to each new one. */
  outputs: readonly Output[];
  onOpen: (path: string) => void;
  onClose: () => void;
}) {
  const [view, setView] = createSignal<FileView>();
  const [diff, setDiff] = createSignal(false);
  const [follow, setFollow] = createSignal(false);
  const [failed, setFailed] = createSignal<string>();
  const language = new Compartment();
  const merge = new Compartment();
  let pane: HTMLDivElement | undefined;
  let host: HTMLDivElement | undefined;
  let editor: EditorView | undefined;
  // What the editor holds now, so a reload of the same file patches it in place.
  let holding: { path: string; base: string | null } | undefined;
  let loaded: string | null | undefined;
  let seen: string | null | undefined;
  let asks = 0;

  const name = () => props.path.split(/[\\/]/).pop() ?? props.path;
  const comparing = (base: string | null): Extension =>
    diff()
      ? unifiedMergeView({
          original: base ?? "",
          mergeControls: false,
          gutter: true,
        })
      : [];

  const show = (path: string, read: FileView) => {
    setView(read);
    if (read.kind !== "text" || !editor) return;
    const text = lines(read.text);
    const base = read.base === null ? null : lines(read.base);
    const same = holding?.path === path;
    if (!same) {
      const mode = LANGUAGES[languageOf(path) ?? ""];
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: text },
        effects: [
          language.reconfigure(mode ? mode() : []),
          merge.reconfigure(comparing(base)),
        ],
        selection: { anchor: 0 },
        scrollIntoView: true,
      });
    } else {
      if (holding?.base !== base) {
        editor.dispatch({ effects: merge.reconfigure(comparing(base)) });
      }
      const change = changeBetween(editor.state.doc.toString(), text);
      if (change) {
        const place = editor.scrollSnapshot();
        const changes = editor.state.changes(change);
        editor.dispatch({ changes });
        editor.dispatch({ effects: place.map(changes) });
      }
    }
    holding = { path, base };
  };

  const load = async (path: string) => {
    const ask = ++asks;
    try {
      // The stamp first, so a write landing during the read shows up as a change on the next poll.
      const stamp = await outputStamp(props.tab, path);
      const read = await readOutput(props.tab, path);
      if (ask !== asks) return;
      loaded = stamp;
      seen = stamp;
      setFailed(undefined);
      show(path, read);
    } catch (error) {
      if (ask === asks) setFailed(`Could not read it: ${String(error)}`);
    }
  };

  let polling = false;
  const poll = async () => {
    if (polling) return;
    polling = true;
    try {
      const path = props.path;
      const now = await outputStamp(props.tab, path);
      if (path !== props.path) return;
      // Read once the stamp has held for a whole poll, so a file mid-write is not shown half done.
      if (now !== loaded && now === seen) {
        await load(path);
        return;
      }
      seen = now;
    } catch {
      // The next poll asks again; a lasting failure shows on the next read.
    } finally {
      polling = false;
    }
  };

  onMount(() => {
    if (!host) return;
    editor = new EditorView({
      parent: host,
      state: EditorState.create({
        extensions: [
          EditorState.readOnly.of(true),
          lineNumbers(),
          highlightSpecialChars(),
          syntaxHighlighting(COLOURS),
          language.of([]),
          merge.of([]),
          THEME,
        ],
      }),
    });
    const timer = setInterval(() => {
      void poll();
    }, POLL_MS);
    pane?.focus();
    onCleanup(() => {
      clearInterval(timer);
      editor?.destroy();
      editor = undefined;
    });
  });

  createEffect(
    on(
      () => props.path,
      (path) => {
        void load(path);
      },
    ),
  );

  // The chat's own write reloads at once; the poll catches every other change.
  createEffect(
    on(
      () =>
        [
          props.path,
          props.outputs.find((o) => o.path === props.path)?.at,
        ] as const,
      ([path, at], before) => {
        if (before?.[0] === path && before[1] !== undefined && at !== before[1])
          void load(path);
      },
    ),
  );

  const newest = () => props.outputs.at(0)?.path;
  createEffect(
    on([follow, newest], ([following, path]) => {
      if (following && path !== undefined && path !== props.path)
        props.onOpen(path);
    }),
  );

  const toggleDiff = () => {
    setDiff((was) => !was);
    const base = holding?.base ?? null;
    editor?.dispatch({ effects: merge.reconfigure(comparing(base)) });
  };

  const sameAsHead = () => {
    const read = view();
    return (
      read?.kind === "text" &&
      read.base !== null &&
      lines(read.base) === lines(read.text)
    );
  };
  const untracked = () => {
    const read = view();
    return read?.kind === "text" && read.base === null;
  };

  const vsCode = () => {
    openOutput(props.tab, props.path).catch((error: unknown) => {
      setFailed(`Could not open it: ${String(error)}`);
    });
  };

  return (
    <div
      class="file-pane"
      tabIndex={-1}
      ref={(el) => {
        pane = el;
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          props.onClose();
        }
      }}
    >
      <div class="service-bar file-bar">
        <span class="split-title file-name" title={props.path}>
          {name()}
        </span>
        <Show when={diff() && untracked()}>
          <span class="file-note">Not in git: all new</span>
        </Show>
        <Show when={diff() && sameAsHead()}>
          <span class="file-note">No change since HEAD</span>
        </Show>
        <span class="spacer" />
        <button
          type="button"
          class="secondary"
          aria-pressed={diff()}
          title="Show the change against git HEAD"
          onClick={toggleDiff}
        >
          Diff
        </button>
        <button
          type="button"
          class="secondary"
          aria-pressed={follow()}
          title="Switch to each new file the chat writes"
          onClick={() => setFollow((was) => !was)}
        >
          Follow
        </button>
        <button
          type="button"
          class="secondary"
          onClick={() => {
            props.onClose();
          }}
        >
          Close
        </button>
      </div>
      <Show when={failed()}>
        {(message) => <p class="file-message">{message()}</p>}
      </Show>
      <Show when={view()}>
        {(read) => (
          <Show when={refusal(read())}>
            {(why) => (
              <div class="file-message">
                <p>{why()}</p>
                <Show when={read().kind !== "gone"}>
                  <button type="button" class="secondary" onClick={vsCode}>
                    Open in VS Code
                  </button>
                </Show>
              </div>
            )}
          </Show>
        )}
      </Show>
      <div
        class="file-code"
        hidden={view()?.kind !== "text"}
        ref={(el) => {
          host = el;
        }}
      />
    </div>
  );
}
