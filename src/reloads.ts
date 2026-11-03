import { isReloadKey, type Platform, PLATFORM } from "./keys";

// Inputs of these types take no typed text, so their menu is the page's, with Reload in it.
const NOT_TEXT = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);
const EDITABLE_CONTENT = new Set(["", "true", "plaintext-only"]);

// Text boxes keep their own menu (cut, copy, paste); xterm's input is one of them.
function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  const field = target.closest("input, textarea, [contenteditable]");
  if (field instanceof HTMLInputElement) return !NOT_TEXT.has(field.type);
  if (field instanceof HTMLTextAreaElement) return true;
  const content = field?.getAttribute("contenteditable") ?? null;
  return content !== null && EDITABLE_CONTENT.has(content.toLowerCase());
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
