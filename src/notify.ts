import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  type ActionPerformed,
  isPermissionGranted,
  onAction,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { createSignal, onCleanup, onMount } from "solid-js";

import {
  type Alert,
  alertFor,
  type Board,
  type Change,
  isQuiet,
  resetText,
} from "./board";
import type { TabId, Tree } from "./tree";

export interface Toast {
  title: string;
  body: string;
}

/** Names the tab and its project only: no chat text, and a title in the default shape drops its account. */
export function toastFor(
  alert: Alert,
  tree: Tree,
  tab: TabId,
  locale?: string,
): Toast | undefined {
  const found = tree.tabs.find((t) => t.id === tab);
  if (!found) return undefined;
  // Matched by shape, not by the current name, since a saved title keeps an account's old name.
  const name = found.title.startsWith(`${found.launch.label} · `)
    ? found.launch.label
    : found.title;
  const project = tree.projects.find((p) => p.id === found.projectId)?.name;
  return {
    title: project === undefined ? name : `${name} · ${project}`,
    body: bodyOf(alert, locale),
  };
}

function bodyOf(alert: Alert, locale: string | undefined): string {
  switch (alert.kind) {
    case "finished":
      return "Waiting on you";
    case "limited":
      return alert.resetsAt === null
        ? "Hit its limit"
        : `Hit its limit, resets ${resetText(alert.resetsAt, locale)}`;
    case "ended":
      return "The chat ended";
  }
}

/** The tab a clicked notification was for, when it still exists. */
export function clickedTab(
  performed: ActionPerformed,
  tree: Tree,
): TabId | undefined {
  const tab = performed.notification?.extra.tab;
  return typeof tab === "number" && tree.tabs.some((t) => t.id === tab)
    ? tab
    : undefined;
}

interface AlertsApp {
  tree: () => Tree;
  board: () => Board;
  shown: () => TabId | undefined;
  alertsOn: () => boolean;
  open: (tab: TabId) => void;
}

async function permitted(): Promise<boolean> {
  try {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === "granted";
  } catch (error: unknown) {
    console.error("notification permission unavailable:", error);
    return false;
  }
}

function logged(what: string): (error: unknown) => undefined {
  return (error) => {
    console.error(`${what}:`, error);
  };
}

/** Returns the call to make with each change, before the board takes it. */
export function createNotifier(
  app: AlertsApp,
): (tab: TabId, change: Change) => void {
  const [focused, setFocused] = createSignal(true);
  // Asked once, on the first alert; a refusal leaves alerts silently off.
  let allowed: Promise<boolean> | undefined;

  onMount(() => {
    const win = getCurrentWindow();
    let heard = false;
    win.isFocused().then((now) => {
      if (!heard) setFocused(now);
    }, logged("could not read window focus"));
    const focus = win
      .onFocusChanged(({ payload }) => {
        heard = true;
        setFocused(payload);
      })
      .catch(logged("could not follow window focus"));
    const clicks = onAction((performed) => {
      const tab = clickedTab(performed, app.tree());
      if (tab === undefined) return;
      app.open(tab);
      win
        .unminimize()
        .then(() => win.setFocus())
        .catch(logged("could not bring the window forward"));
    }).catch(logged("could not follow notification clicks"));
    onCleanup(() => {
      void focus.then((stop) => stop?.());
      void clicks.then((listener) => listener?.unregister());
    });
  });

  const view = () => ({
    shown: app.shown(),
    focused: focused(),
    alertsOn: app.alertsOn(),
  });
  return (tab, change) => {
    const alert = alertFor(app.board(), tab, change, view());
    if (!alert) return;
    allowed ??= permitted();
    void allowed.then((ok) => {
      // Checked again, since the first permission answer can arrive after you opened the tab.
      const toast = toastFor(alert, app.tree(), tab);
      if (ok && toast && !isQuiet(view(), tab)) {
        sendNotification({ ...toast, extra: { tab } });
      }
    });
  };
}

// Asked on the first warning, apart from the tab alerts' own ask.
let warningsAllowed: Promise<boolean> | undefined;

/** A low-limit warning; a click opens the tab it names, as a tab alert's does. */
export function sendWarning(toast: Toast, tab: TabId): void {
  warningsAllowed ??= permitted();
  void warningsAllowed.then((ok) => {
    if (ok) sendNotification({ ...toast, extra: { tab } });
  });
}
