import { createSignal, For, onCleanup, onMount, Show, untrack } from "solid-js";

import { backdropDismiss } from "./backdrop";
import { pickFolder, type Profiles, savedColor } from "./accounts";
import {
  closeFind,
  type FindBatch,
  type FoundChat,
  merged,
  startFind,
  whenText,
} from "./chatSearch";
import { AccountDot, CloseIcon, ProgramIcon, SearchIcon } from "./icons";
import { keepInside } from "./ShortcutsSheet";
import { matchTabs, type TabEntry } from "./tabSearch";
import { folderName, type TabId } from "./tree";

// Long enough to skip the searches between quick keystrokes, short enough to feel live.
const TYPING_MS = 120;

/** One list for open tabs and every account's chats: Enter on a tab shows it, on a chat opens it in its folder on its account. */
export function FindChat(props: {
  profiles: Profiles;
  /** Every open tab, in the sidebar's order. */
  tabs: TabEntry[];
  onSwitch: (tab: TabId) => void;
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

  // A chat already open in a tab shows once, as that tab, even when only its words matched.
  const shownTabs = () => {
    const matched = matchTabs(props.tabs, query());
    const viaChats = chats()
      .map((chat) => chat.openIn)
      .filter((id): id is number => id !== null)
      .filter((id) => !matched.some((t) => t.id === id))
      .map((id) => props.tabs.find((t) => t.id === id))
      .filter((tab): tab is TabEntry => tab !== undefined);
    return [...matched, ...viaChats];
  };
  const shownChats = () => chats().filter((chat) => chat.openIn === null);
  const rowCount = () => shownTabs().length + shownChats().length;
  const pick = (at: number) => {
    const tabs = shownTabs();
    const tab = tabs[at];
    if (tab) props.onSwitch(tab.id);
    else open(shownChats()[at - tabs.length]);
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
    const count = rowCount();
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
        pick(selected());
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
      {...backdropDismiss(() => {
        props.onClose();
      })}
    >
      <div
        class="sheet find-chat"
        role="dialog"
        aria-modal="true"
        aria-label="Search tabs and chats"
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
              placeholder="Tabs, or words from a chat, or its id"
              aria-label="Search tabs and chats"
              value={query()}
              onInput={(event) => {
                const text = event.currentTarget.value;
                setQuery(text);
                setSelected(0);
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
          aria-label="Tabs and chats"
          ref={(el) => {
            list = el;
          }}
        >
          <Show when={shownTabs().length > 0}>
            <li class="find-group" role="presentation">
              Open tabs
            </li>
          </Show>
          <For each={shownTabs()}>
            {(tab, at) => (
              <li
                role="option"
                aria-selected={at() === selected()}
                class="find-row find-tab"
                classList={{ selected: at() === selected() }}
                title={tab.folder ?? undefined}
                onPointerMove={() => setSelected(at())}
                onClick={() => {
                  props.onSwitch(tab.id);
                }}
              >
                <div class="find-line">
                  <ProgramIcon program={tab.program} size={13} />
                  <span class="find-title">{tab.title}</span>
                </div>
                <div class="find-line find-meta">
                  <span>{tab.project}</span>
                  <Show when={tab.account}>
                    {(name) => (
                      <>
                        <AccountDot
                          id={tab.accountId ?? 0}
                          color={savedColor(props.profiles, tab.accountId ?? 0)}
                        />
                        <span>{name()}</span>
                      </>
                    )}
                  </Show>
                  <Show when={tab.folder}>
                    {(folder) => (
                      <span class="find-folder">{folderName(folder())}</span>
                    )}
                  </Show>
                </div>
              </li>
            )}
          </For>
          <li class="find-group" role="presentation">
            Chats
          </li>
          <For each={shownChats()}>
            {(chat, index) => (
              <li
                role="option"
                aria-selected={shownTabs().length + index() === selected()}
                class="find-row"
                classList={{
                  selected: shownTabs().length + index() === selected(),
                }}
                title={chat.folder ?? undefined}
                onPointerMove={() => setSelected(shownTabs().length + index())}
                onClick={() => {
                  open(chat);
                }}
              >
                <div class="find-line">
                  <span class="find-title">{chat.title}</span>
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
                ? `${String(chats().length)} chats so far…`
                : "Searching chats…"
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
