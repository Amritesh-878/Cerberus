import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { backdropDismiss } from "./backdrop";
import { ChevronIcon, MoreIcon, PlusIcon, ServicesIcon } from "./icons";
import { placeBox } from "./place";
import {
  type Action,
  isOn,
  overallPhase,
  type Phase,
  phaseText,
  type ServiceRow,
  type ServicesView,
  servicesSummary,
} from "./services";

/** The host and port, which is what tells two addresses apart in a narrow row. */
export function shortUrl(url: string): string {
  return url.replace(/^https?:\/\//, "").replace(/\/$/, "");
}

/** The menu's items for a service in `phase`, in order. */
export function menuItems(phase: Phase): (Action | "output" | "edit")[] {
  const run: Action[] = isOn(phase) ? ["stop", "restart"] : ["start"];
  return [...run, "output", "edit", "remove"];
}

const LABELS = {
  start: "Start",
  stop: "Stop",
  restart: "Restart",
  output: "Show output",
  edit: "Edit…",
  remove: "Remove…",
};

function ServiceMenu(props: {
  service: ServiceRow;
  at: DOMRect;
  onPick: (item: Action | "output" | "edit") => void;
  onDismiss: () => void;
}) {
  const [confirming, setConfirming] = createSignal(false);
  const [place, setPlace] = createSignal({ left: 0, top: 0 });
  let box: HTMLDivElement | undefined;
  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onDismiss();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
    });
    const size = {
      width: box?.offsetWidth ?? 0,
      height: box?.offsetHeight ?? 0,
    };
    setPlace(
      placeBox(props.at, size, { width: innerWidth, height: innerHeight }),
    );
    box?.querySelector<HTMLButtonElement>("button")?.focus();
  });

  return (
    <div
      class="popover-backdrop"
      {...backdropDismiss(() => {
        props.onDismiss();
      })}
    >
      <div
        class="popover menu service-menu"
        role="menu"
        aria-label={`${props.service.name} menu`}
        style={{
          left: `${String(place().left)}px`,
          top: `${String(place().top)}px`,
        }}
        ref={(el) => {
          box = el;
        }}
      >
        <Show
          when={confirming()}
          fallback={
            <For each={menuItems(props.service.phase)}>
              {(item) => (
                <button
                  type="button"
                  class="menu-item"
                  role="menuitem"
                  onClick={() => {
                    if (item === "remove") setConfirming(true);
                    else props.onPick(item);
                  }}
                >
                  <span class="menu-text">{LABELS[item]}</span>
                </button>
              )}
            </For>
          }
        >
          <div
            class="menu-confirm"
            role="alertdialog"
            ref={(el) => {
              queueMicrotask(() => el.querySelector("button")?.focus());
            }}
          >
            <p>
              Remove {props.service.name}?
              {isOn(props.service.phase) ? " It stops first." : ""}
            </p>
            <div class="confirm-actions">
              <button
                type="button"
                class="danger"
                onClick={() => {
                  props.onPick("remove");
                }}
              >
                Remove
              </button>
              <button
                type="button"
                onClick={() => {
                  props.onDismiss();
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}

function ServiceItem(props: {
  service: ServiceRow;
  onAct: (action: Action) => void;
  onOutput: () => void;
  onEdit: () => void;
  onOpenUrl: () => void;
}) {
  const [menu, setMenu] = createSignal<DOMRect>();
  const [copied, setCopied] = createSignal(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => {
    clearTimeout(copiedTimer);
  });
  const copy = (url: string) => {
    navigator.clipboard.writeText(url).then(
      () => {
        setCopied(true);
        clearTimeout(copiedTimer);
        copiedTimer = setTimeout(() => setCopied(false), 1200);
      },
      (error: unknown) => {
        console.error("copy failed:", error);
      },
    );
  };
  const title = () =>
    [
      phaseText(props.service.phase),
      props.service.folder ?? "Home folder",
      props.service.problem,
    ]
      .filter(Boolean)
      .join("\n");

  return (
    <li class="service-row" data-phase={props.service.phase}>
      <span
        class={`dot service-dot ${props.service.phase}`}
        role="img"
        aria-label={phaseText(props.service.phase)}
      />
      <button
        type="button"
        class="service-name"
        title={title()}
        onClick={() => {
          props.onOutput();
        }}
      >
        {props.service.name}
      </button>
      <Show when={props.service.phase === "crashed"}>
        <button
          type="button"
          class="service-chip"
          title="It stopped five times in two minutes"
          onClick={() => {
            props.onAct("restart");
          }}
        >
          Restart
        </button>
      </Show>
      <button
        type="button"
        class="icon-button service-more"
        aria-label={`${props.service.name} menu`}
        aria-haspopup="menu"
        aria-expanded={menu() !== undefined}
        onClick={(event) => {
          setMenu(event.currentTarget.getBoundingClientRect());
        }}
      >
        <MoreIcon />
      </button>
      <Show when={props.service.url}>
        {(url) => (
          <span class="service-url">
            <button
              type="button"
              class="service-chip link"
              title={`Open ${url()} in the browser`}
              onClick={() => {
                props.onOpenUrl();
              }}
            >
              {shortUrl(url())}
            </button>
            <button
              type="button"
              class="service-chip"
              aria-label={`Copy ${url()}`}
              title="Copy the address"
              onClick={() => {
                copy(url());
              }}
            >
              {copied() ? "Copied" : "Copy"}
            </button>
          </span>
        )}
      </Show>
      <Show when={menu()}>
        {(at) => (
          <ServiceMenu
            service={props.service}
            at={at()}
            onDismiss={() => {
              setMenu(undefined);
            }}
            onPick={(item) => {
              setMenu(undefined);
              if (item === "output") props.onOutput();
              else if (item === "edit") props.onEdit();
              else props.onAct(item);
            }}
          />
        )}
      </Show>
    </li>
  );
}

/** The collapsed sidebar's one icon for every service, with the dot of the state that most needs a look. */
export function ServicesRailIcon(props: {
  view: ServicesView;
  onOpen: () => void;
}) {
  return (
    <Show when={overallPhase(props.view.services)}>
      {(phase) => (
        <button
          type="button"
          class="rail-project rail-services"
          aria-label={`Services, ${servicesSummary(props.view.services)}`}
          title={`Services: ${servicesSummary(props.view.services)}`}
          onClick={() => {
            props.onOpen();
          }}
        >
          <ServicesIcon />
          <span class={`dot service-dot rail-dot ${phase()}`} />
        </button>
      )}
    </Show>
  );
}

/** The sidebar's Services: one row per service, folded to its title when asked. */
export function ServicesSection(props: {
  view: ServicesView;
  onFold: (folded: boolean) => void;
  onAdd: () => void;
  onAct: (id: number, action: Action) => void;
  onOutput: (id: number) => void;
  onEdit: (service: ServiceRow) => void;
  onOpenUrl: (id: number) => void;
}) {
  const running = () =>
    props.view.services.filter((s) => s.phase === "running").length;
  return (
    <section class="services" classList={{ folded: props.view.folded }}>
      <div class="services-head">
        <button
          type="button"
          class="services-title"
          aria-expanded={!props.view.folded}
          onClick={() => {
            props.onFold(!props.view.folded);
          }}
        >
          <ChevronIcon />
          Services
          <Show when={props.view.folded && props.view.services.length > 0}>
            <span class="services-count">
              {running()}/{props.view.services.length}
            </span>
          </Show>
        </button>
        <button
          type="button"
          class="icon-button"
          aria-label="New service"
          title="New service"
          onClick={() => {
            props.onAdd();
          }}
        >
          <PlusIcon />
        </button>
      </div>
      <Show when={!props.view.folded}>
        <ul class="services-list">
          <For
            each={props.view.services}
            fallback={
              <li class="services-empty">
                Tunnels and dev servers that keep running without a tab.
              </li>
            }
          >
            {(service) => (
              <ServiceItem
                service={service}
                onAct={(action) => {
                  props.onAct(service.id, action);
                }}
                onOutput={() => {
                  props.onOutput(service.id);
                }}
                onEdit={() => {
                  props.onEdit(service);
                }}
                onOpenUrl={() => {
                  props.onOpenUrl(service.id);
                }}
              />
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
