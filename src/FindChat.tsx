import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import { pickFolder, type Profiles, savedColor } from "./accounts";
import {
  closeFind,
  type FindBatch,
  type FoundChat,
  merged,
  startFind,
  whenText,
} from "./chatSearch";
import { AccountDot, CloseIcon, SearchIcon } from "./icons";
import { keepInside } from "./ShortcutsSheet";
import { folderName } from "./tree";

// Long enough to skip the searches between quick keystrokes, short enough to feel live.
const TYPING_MS = 120;

/** Searches every account's chats; Enter or a click opens one, in its folder on its account. */
export function FindChat(props: {
  profiles: Profiles;
  /** Opens the chat, in `folder` when its own is gone; the dialog closes once it resolves. */
  onOpen: (chat: FoundChat, folder?: string) => Promise<void>;
  onClose: () => void;
}) {
  const [query, setQuery] = createSignal("");
  const [chats, setChats] = createSignal<FoundChat[]>([]);
  const [done, setDone] = createSignal<number>();
  const [selected, setSelected] = createSignal(0);
  const [gone, setGone] = createSignal<FoundChat>();
  const [error, setError] = createSignal<string>();
  const [opening, setOpening] = createSignal(false);
  // The newest search seen; a batch from an older one is dropped, a newer one starts the list over.
  let newest = 0;
  let typing: ReturnType<typeof setTimeout> | undefined;

  const onBatch = (batch: FindBatch) => {
    if (batch.search < newest) return;
    if (batch.search > newest) {
      newest = batch.search;
      setChats([]);
      setDone(undefined);
      setSelected(0);
    }
    if (batch.kind === "found") setChats((list) => merged(list, batch.chats));
    else setDone(batch.searched);
  };
  const search = (text: string) => {
    startFind(text, onBatch).then(
      (number) => {
        if (number > newest) {
          newest = number;
          setChats([]);
          setDone(undefined);
          setSelected(0);
        }
      },
      (failure: unknown) => setError(`Could not search: ${String(failure)}`),
    );
  };

  const accountName = (id: number | null) =>
    id === null
      ? "Default account"
      : (props.profiles.accounts.find((a) => a.id === id)?.name ??
        "Removed account");
  const open = (chat: FoundChat | undefined, folder?: string) => {
    if (!chat || opening()) return;
    if (!chat.folderExists && chat.openIn === null && folder === undefined) {
      setGone(chat);
      return;
    }
    setOpening(true);
    setError(undefined);
    props
      .onOpen(chat, folder)
      .then(props.onClose, (failure: unknown) =>
        setError(`Could not open the chat: ${String(failure)}`),
      )
      .finally(() => setOpening(false));
  };
  const choose = (chat: FoundChat) => {
    pickFolder(untrack(() => chat.folder) ?? undefined).then(
      (folder) => {
        if (folder !== null) open(chat, folder);
      },
      (failure: unknown) =>
        setError(`Could not open the picker: ${String(failure)}`),
    );
  };

  let sheet: HTMLDivElement | undefined;
  let list: HTMLUListElement | undefined;
  const move = (by: number) => {
    const count = chats().length;
    if (count === 0) return;
    setSelected((at) => Math.max(0, Math.min(count - 1, at + by)));
    queueMicrotask(() => {
      list
        ?.querySelector<HTMLElement>('[aria-selected="true"]')
        ?.scrollIntoView({ block: "nearest" });
    });
  };
  onMount(() => {
    const opener = document.activeElement;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (gone()) setGone(undefined);
        else props.onClose();
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        move(event.key === "ArrowDown" ? 1 : -1);
      } else if (
        event.key === "Enter" &&
        !gone() &&
        !event.isComposing &&
        // A focused button, such as Close, does what it says instead.
        !(event.target instanceof HTMLButtonElement)
      ) {
        event.preventDefault();
        open(chats()[selected()]);
      } else if (event.key === "Tab" && sheet && keepInside(sheet, event)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey, true);
    search("");
    onCleanup(() => {
      window.removeEventListener("keydown", onKey, true);
      clearTimeout(typing);
      void closeFind();
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    });
    sheet?.querySelector<HTMLInputElement>("input")?.focus();
  });

  return (
    <div
      class="sheet-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        class="sheet find-chat"
        role="dialog"
        aria-modal="true"
        aria-label="Find a chat"
        tabIndex={-1}
        ref={(el) => {
          sheet = el;
        }}
      >
        <div class="find-head">
          <label class="search find-box">
            <SearchIcon />
            <input
              type="search"
              placeholder="Find a chat on any account"
              aria-label="Find a chat"
              value={query()}
              onInput={(event) => {
                const text = event.currentTarget.value;
                setQuery(text);
                setGone(undefined);
                clearTimeout(typing);
                typing = setTimeout(() => {
                  search(text);
                }, TYPING_MS);
              }}
            />
          </label>
          <button
            type="button"
            class="icon-button"
            aria-label="Close"
            onClick={() => {
              props.onClose();
            }}
          >
            <CloseIcon />
          </button>
        </div>
        <Show when={gone()}>
          {(chat) => (
            <div class="find-gone" role="alert">
              <p title={chat().folder ?? undefined}>
                Its folder, {folderName(chat().folder ?? "") || "unknown"}, is
                gone. Choose a folder to continue the chat there.
              </p>
              <div class="find-gone-actions">
                <button
                  type="button"
                  class="secondary"
                  onClick={() => {
                    setGone(undefined);
                  }}
                >
                  Back
                </button>
                <button
                  type="button"
                  disabled={opening()}
                  onClick={() => {
                    choose(chat());
                  }}
                >
                  Choose a folder…
                </button>
              </div>
            </div>
          )}
        </Show>
        <ul
          class="find-list"
          role="listbox"
          aria-label="Chats"
          ref={(el) => {
            list = el;
          }}
        >
          <For each={chats()}>
            {(chat, at) => (
              <li
                role="option"
                aria-selected={at() === selected()}
                class="find-row"
                classList={{ selected: at() === selected() }}
                title={chat.folder ?? undefined}
                onPointerMove={() => setSelected(at())}
                onClick={() => {
                  open(chat);
                }}
              >
                <div class="find-line">
                  <span class="find-title">{chat.title}</span>
                  <Show when={chat.openIn !== null}>
                    <span class="find-tag">open</span>
                  </Show>
                  <span class="find-when">{whenText(chat.last)}</span>
                </div>
                <div class="find-line find-meta">
                  <Show when={chat.account !== null}>
                    <AccountDot
                      id={chat.account ?? 0}
                      color={savedColor(props.profiles, chat.account ?? 0)}
                    />
                  </Show>
                  <span>{accountName(chat.account)}</span>
                  <span
                    class="find-folder"
                    classList={{ gone: !chat.folderExists }}
                  >
                    {chat.folder === null
                      ? "No folder"
                      : folderName(chat.folder)}
                    {chat.folderExists ? "" : " (gone)"}
                  </span>
                </div>
                <Show when={chat.snippet}>
                  {(snippet) => <p class="find-snippet">{snippet()}</p>}
                </Show>
              </li>
            )}
          </For>
        </ul>
        <p class="find-status" aria-live="polite">
          <Show
            when={done() !== undefined}
            fallback={
              chats().length > 0
                ? `${String(chats().length)} so far…`
                : "Searching…"
            }
          >
            {chats().length === 0
              ? query().trim() === ""
                ? "No chats yet"
                : "No chats match"
              : `${String(chats().length)} of ${String(done())} chats`}
          </Show>
        </p>
        <Show when={error()}>
          {(message) => (
            <p class="error" role="alert">
              {message()}
            </p>
          )}
        </Show>
      </div>
    </div>
  );
}
