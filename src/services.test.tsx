import { emit as emitEvent } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";

import { cleanup, click, field, renderApp, SAVED, settle } from "./harness";
import {
  BLANK,
  devServerDraft,
  overallPhase,
  portOf,
  type ServiceRow,
  type ServicesView,
  servicesSummary,
  tunnelDraft,
} from "./services";
import { menuItems, shortUrl } from "./ServicesSection";

afterEach(cleanup);

const TUNNEL: ServiceRow = {
  id: 1,
  name: "tunnel",
  phase: "running",
  url: "https://fake-words-here.trycloudflare.com",
  folder: null,
  shell: null,
  startWithApp: true,
  restart: true,
  problem: null,
};

const VIEW: ServicesView = {
  folded: false,
  services: [
    TUNNEL,
    { ...TUNNEL, id: 2, name: "api", phase: "crashed", url: null },
    { ...TUNNEL, id: 3, name: "docs", phase: "stopped", url: null },
  ],
};

function servicesCore() {
  const calls: { cmd: string; args: unknown }[] = [];
  const record =
    (cmd: string, answer: unknown = VIEW) =>
    (args: unknown) => {
      calls.push({ cmd, args });
      return answer;
    };
  return {
    calls,
    of: (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args),
    core: {
      services_view: () => VIEW,
      list_programs: () => [
        {
          id: "powershell",
          label: "PowerShell",
          program: String.raw`C:\ps.exe`,
          args: [],
          imagePasteKey: null,
          isShell: true,
          installCommand: null,
          installShell: null,
        },
      ],
      service_add: record("service_add"),
      service_update: record("service_update"),
      service_start: record("service_start"),
      service_stop: record("service_stop"),
      service_restart: record("service_restart"),
      service_remove: record("service_remove"),
      services_fold: record("services_fold", { ...VIEW, folded: true }),
      service_command: record("service_command", "npm run dev -- --port 9"),
      service_watch: record("service_watch", 7),
      service_unwatch: record("service_unwatch", null),
      service_open_url: record("service_open_url", null),
    },
  };
}

function rows(root: HTMLElement): string[] {
  return [...root.querySelectorAll(".service-row")].map((row) => {
    const phase = row.getAttribute("data-phase") ?? "";
    const name = row.querySelector(".service-name")?.textContent ?? "";
    const chips = [...row.querySelectorAll(".service-chip")]
      .map((c) => c.textContent)
      .join(",");
    return `${name} ${phase} ${chips}`.trim();
  });
}

function row(root: HTMLElement, name: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>(".service-row")].find(
    (r) => r.querySelector(".service-name")?.textContent === name,
  );
  if (!found) throw new Error(`no ${name} service`);
  return found;
}

function menuOf(root: HTMLElement, name: string): HTMLElement {
  row(root, name).querySelector<HTMLButtonElement>(".service-more")?.click();
  const menu = root.querySelector<HTMLElement>(".service-menu");
  if (!menu) throw new Error("no menu");
  return menu;
}

function input(root: HTMLElement, label: string): HTMLInputElement {
  const found = [...root.querySelectorAll("label")].find(
    (l) => l.querySelector("span")?.textContent === label,
  );
  const box = found?.querySelector("input");
  if (!box) throw new Error(`no ${label} box`);
  return box;
}

function type(box: HTMLInputElement, text: string): void {
  box.value = text;
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

test("each service shows its state, its address and a Restart when crashed", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  expect(rows(root)).toEqual([
    "tunnel running fake-words-here.trycloudflare.com,Copy",
    "api crashed Restart",
    "docs stopped",
  ]);
  click(row(root, "api"), "Restart");
  click(row(root, "tunnel"), "fake-words-here.trycloudflare.com");
  await settle();

  expect(of("service_restart")).toEqual([{ id: 2 }]);
  expect(of("service_open_url")).toEqual([{ id: 1 }]);
});

test("the core's event redraws the rows, and the title folds the section and saves it", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  await emitEvent("services", {
    ...VIEW,
    services: [{ ...TUNNEL, phase: "restarting", url: null }],
  });
  await settle();
  expect(rows(root)).toEqual(["tunnel restarting"]);
  click(root.querySelector(".services") ?? root, "Services");
  await settle();

  expect(of("services_fold")).toEqual([{ folded: true }]);
  expect(root.querySelector(".service-row")).toBeNull();
  expect(root.querySelector(".services-count")?.textContent).toBe("1/3");
});

test("the menu offers Stop for a running service and Start for a stopped one", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  const items = (menu: HTMLElement) =>
    [...menu.querySelectorAll(".menu-item")].map((i) => i.textContent);
  expect(items(menuOf(root, "tunnel"))).toEqual([
    "Stop",
    "Restart",
    "Show output",
    "Edit…",
    "Remove…",
  ]);
  click(root.querySelector(".service-menu") ?? root, "Stop");
  await settle();
  expect(items(menuOf(root, "docs"))[0]).toBe("Start");
  click(root.querySelector(".service-menu") ?? root, "Start");
  await settle();

  expect(of("service_stop")).toEqual([{ id: 1 }]);
  expect(of("service_start")).toEqual([{ id: 3 }]);
});

test("Remove asks first, and only Remove removes", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  click(menuOf(root, "tunnel"), "Remove…");
  await settle();
  expect(
    root.querySelector(".service-menu .menu-confirm")?.textContent,
  ).toContain("Remove tunnel? It stops first.");
  click(root.querySelector(".menu-confirm") ?? root, "Cancel");
  await settle();
  expect(of("service_remove")).toEqual([]);
  expect(root.querySelector(".service-menu")).toBeNull();

  click(menuOf(root, "docs"), "Remove…");
  await settle();
  click(root.querySelector(".menu-confirm") ?? root, "Remove");
  await settle();
  expect(of("service_remove")).toEqual([{ id: 3 }]);
});

test("the tunnel preset asks for a port and fills the command, then Save adds it", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  root.querySelector<HTMLButtonElement>('[aria-label="New service"]')?.click();
  await settle();
  const form = root.querySelector<HTMLElement>(".service-form") ?? root;
  click(form, "cloudflared tunnel");
  await settle();
  type(input(root, "Local port"), "8080");
  click(form, "Fill in");
  await settle();

  expect(input(root, "Name").value).toBe("tunnel 8080");
  expect(input(root, "Command").value).toBe(
    "cloudflared tunnel --url http://localhost:8080",
  );
  click(form, "Save");
  await settle();
  expect(of("service_add")).toEqual([
    {
      draft: {
        name: "tunnel 8080",
        command: "cloudflared tunnel --url http://localhost:8080",
        folder: null,
        shell: null,
        startWithApp: false,
        restart: true,
      },
    },
  ]);
  expect(root.querySelector(".service-form")).toBeNull();
});

test("a bad port is refused, and Dev server fills npm run dev", async () => {
  const { core } = servicesCore();
  const root = await renderApp(() => SAVED, core);
  root.querySelector<HTMLButtonElement>('[aria-label="New service"]')?.click();
  await settle();
  const form = root.querySelector<HTMLElement>(".service-form") ?? root;

  click(form, "cloudflared tunnel");
  await settle();
  type(input(root, "Local port"), "99999");
  click(form, "Fill in");
  await settle();
  expect(form.querySelector(".error")?.textContent).toContain("1 to 65535");
  click(form, "Dev server");
  await settle();

  expect(input(root, "Name").value).toBe("dev server");
  expect(input(root, "Command").value).toBe("npm run dev");
});

test("Edit reads the command from the core and saves the change to that service", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  click(menuOf(root, "docs"), "Edit…");
  await settle();
  expect(of("service_command")).toEqual([{ id: 3 }]);
  expect(input(root, "Command").value).toBe("npm run dev -- --port 9");
  type(input(root, "Name"), "docs site");
  click(root.querySelector(".service-form") ?? root, "Save");
  await settle();

  expect(of("service_update")).toEqual([
    {
      id: 3,
      draft: {
        name: "docs site",
        command: "npm run dev -- --port 9",
        folder: null,
        shell: null,
        startWithApp: true,
        restart: true,
      },
    },
  ]);
});

test("an edit form loaded from the core closes at once, and asks first once edited", async () => {
  const { core } = servicesCore();
  const root = await renderApp(() => SAVED, core);
  const escape = () =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));

  click(menuOf(root, "docs"), "Edit…");
  await settle();
  escape();
  await settle();
  expect(root.querySelector(".service-form")).toBeNull();

  click(menuOf(root, "docs"), "Edit…");
  await settle();
  type(input(root, "Name"), "docs site");
  escape();
  await settle();
  expect(root.querySelector(".discard-ask")).not.toBeNull();
  escape();
  await settle();
  expect(root.querySelector(".discard-ask")).toBeNull();
  expect(input(root, "Name").value).toBe("docs site");
});

test("the first service, whose id is 0, opens its output too", async () => {
  const { core, of } = servicesCore();
  const first = { ...VIEW, services: [{ ...TUNNEL, id: 0 }] };
  const root = await renderApp(() => SAVED, {
    ...core,
    services_view: () => first,
  });

  row(root, "tunnel")
    .querySelector<HTMLButtonElement>(".service-name")
    ?.click();
  await settle();

  expect(root.querySelector(".service-bar")?.textContent).toContain(
    "tunnel output",
  );
  expect(of("service_watch").map((a) => field(a, "id"))).toEqual([0]);
});

test("a click on a service shows its output, and Close stops watching", async () => {
  const { core, of } = servicesCore();
  const root = await renderApp(() => SAVED, core);

  row(root, "tunnel")
    .querySelector<HTMLButtonElement>(".service-name")
    ?.click();
  await settle();

  expect(root.querySelector(".service-bar")?.textContent).toContain(
    "tunnel output",
  );
  expect(of("service_watch").map((a) => field(a, "id"))).toEqual([1]);
  click(root.querySelector(".service-bar") ?? root, "Close");
  await settle();
  expect(root.querySelector(".service-view")).toBeNull();
  expect(of("service_unwatch")).toEqual([{ watch: 7 }]);
});

test("opening Accounts closes a service's output, so the page shows", async () => {
  const { core } = servicesCore();
  const root = await renderApp(() => SAVED, core);
  row(root, "tunnel")
    .querySelector<HTMLButtonElement>(".service-name")
    ?.click();
  await settle();
  expect(root.querySelector(".service-view")).not.toBeNull();

  root.querySelector<HTMLButtonElement>('[aria-label="Accounts"]')?.click();
  await settle();

  expect(root.querySelector(".service-view")).toBeNull();
  expect(root.querySelector(".page")?.hasAttribute("hidden")).toBe(false);
});

test("the helpers behind the form and the rows", () => {
  expect(portOf("5173")).toBe(5173);
  expect(portOf(" 80 ")).toBe(80);
  expect(portOf("0")).toBeUndefined();
  expect(portOf("65536")).toBeUndefined();
  expect(portOf("8o")).toBeUndefined();
  expect(tunnelDraft(3000, { ...BLANK, name: "mine" }).name).toBe("mine");
  expect(devServerDraft(BLANK).command).toBe("npm run dev");
  expect(shortUrl("http://localhost:5173/")).toBe("localhost:5173");
  expect(menuItems("crashed")).toEqual(["start", "output", "edit", "remove"]);
  expect(menuItems("restarting")[0]).toBe("stop");
});

test("the collapsed rail shows one services icon with the dot that most needs a look", async () => {
  const { core, of } = servicesCore();
  const collapsed = { ...SAVED, sidebarCollapsed: true };
  const root = await renderApp(() => collapsed, {
    ...core,
    services_view: () => ({ ...VIEW, folded: true }),
    set_sidebar_collapsed: (args: unknown) => ({
      ...collapsed,
      sidebarCollapsed: field(args, "collapsed") as boolean,
      revision: 1,
    }),
  });

  const icon = root.querySelector<HTMLButtonElement>(".rail-services");
  expect(icon?.getAttribute("aria-label")).toBe(
    "Services, 1 of 3 running, 1 crashed",
  );
  expect(icon?.querySelector(".dot")?.classList.contains("crashed")).toBe(true);
  icon?.click();
  await settle();

  expect(root.querySelector(".rail")).toBeNull();
  expect(of("services_fold")).toEqual([{ folded: false }]);
});

test("with no services the rail shows no services icon", async () => {
  const { core } = servicesCore();
  const root = await renderApp(() => ({ ...SAVED, sidebarCollapsed: true }), {
    ...core,
    services_view: () => ({ folded: false, services: [] }),
  });

  expect(root.querySelector(".rail")).not.toBeNull();
  expect(root.querySelector(".rail-services")).toBeNull();
});

test("the rail's dot and summary for a set of services", () => {
  const row = (phase: ServiceRow["phase"]): ServiceRow => ({
    ...TUNNEL,
    phase,
  });

  expect(overallPhase([])).toBeUndefined();
  expect(overallPhase([row("stopped"), row("running")])).toBe("running");
  expect(overallPhase([row("running"), row("restarting")])).toBe("restarting");
  expect(overallPhase([row("starting"), row("crashed")])).toBe("crashed");
  expect(servicesSummary([row("running"), row("stopped")])).toBe(
    "1 of 2 running",
  );
  expect(servicesSummary([row("restarting")])).toBe(
    "0 of 1 running, 1 restarting",
  );
  expect(
    servicesSummary([row("starting"), row("crashed"), row("restarting")]),
  ).toBe("0 of 3 running, 1 crashed, 1 restarting, 1 starting");
});
