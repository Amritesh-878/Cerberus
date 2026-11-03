import { isReloadKey, type Platform, PLATFORM } from "./keys";

// Text boxes keep their own menu (cut, copy, paste); xterm's input is one of them.
const EDITABLE =
  'input:not([type]), input[type="text"], input[type="search"], textarea, [contenteditable="true"], [contenteditable=""]';

function isEditable(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITABLE) !== null;
}

/** A reload drops the page that owns every tab, so the core ends their programs. */
export function blockReloads(
  target: Window,
  platform: Platform = PLATFORM,
): () => void {
  // Bubble phase, so xterm has already sent Ctrl+R to the shell before the reload is cancelled.
  const onKey = (event: KeyboardEvent) => {
    if (isReloadKey(event, platform)) event.preventDefault();
  };
  // The webview's own menu offers Reload; the app's menus run before this and still open.
  const onMenu = (event: MouseEvent) => {
    if (!isEditable(event.target)) event.preventDefault();
  };
  target.addEventListener("keydown", onKey);
  target.addEventListener("contextmenu", onMenu);
  return () => {
    target.removeEventListener("keydown", onKey);
    target.removeEventListener("contextmenu", onMenu);
  };
}
