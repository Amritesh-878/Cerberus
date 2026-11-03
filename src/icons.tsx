import { Show } from "solid-js";

import { type AccountId, accountColor } from "./accounts";
export function ClaudeIcon(props: { size?: number }) {
  return (
    <svg
      class="icon-claude"
      width={props.size ?? 14}
      height={props.size ?? 14}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      aria-hidden="true"
    >
      <path d="M8 2v12M2 8h12M3.8 3.8l8.4 8.4M12.2 3.8l-8.4 8.4" />
    </svg>
  );
}

export function ShellIcon(props: { size?: number }) {
  return (
    <svg
      class="icon-shell"
      width={props.size ?? 14}
      height={props.size ?? 14}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M3 4.5 6.5 8 3 11.5M8 12h5" />
    </svg>
  );
}

export function ProgramIcon(props: { program: string; size?: number }) {
  return (
    <>
      <Show
        when={props.program === "claude"}
        fallback={<ShellIcon size={props.size} />}
      >
        <ClaudeIcon size={props.size} />
      </Show>
    </>
  );
}

function Stroke(props: { d: string; size?: number; width?: number }) {
  return (
    <svg
      width={props.size ?? 14}
      height={props.size ?? 14}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width={props.width ?? 1.6}
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d={props.d} />
    </svg>
  );
}

export function ChevronIcon() {
  return <Stroke d="m4 6 4 4 4-4" size={12} width={2} />;
}

export function PlusIcon() {
  return <Stroke d="M8 3v10M3 8h10" />;
}

export function CloseIcon() {
  return <Stroke d="m4 4 8 8M12 4l-8 8" size={12} width={1.8} />;
}

export function SearchIcon() {
  return (
    <Stroke
      d="M11.5 11.5 14 14M7 11.5a4.5 4.5 0 1 1 0-9 4.5 4.5 0 0 1 0 9"
      size={13}
    />
  );
}

export function ClockIcon() {
  return (
    <Stroke
      d="M8 4.8V8l2 1.3M8 14A6 6 0 1 1 8 2a6 6 0 0 1 0 12"
      size={11}
      width={1.6}
    />
  );
}

export function FolderIcon() {
  return (
    <Stroke
      d="M2 4.5c0-.6.4-1 1-1h3.2l1.5 1.5H13c.6 0 1 .4 1 1V12c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1z"
      width={1.5}
    />
  );
}

export function PeopleIcon() {
  return (
    <Stroke
      d="M8 8a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5M3 13.5C3.6 11.1 5.6 10 8 10s4.4 1.1 5 3.5"
      width={1.5}
    />
  );
}

/** The app's mark, a terminal window with three tabs, drawn in the text colour. */
export function AppIcon(props: { size?: number }) {
  return (
    <svg
      class="app-icon"
      width={props.size ?? 40}
      height={props.size ?? 40}
      viewBox="0 0 40 40"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <rect x="5" y="9" width="30" height="24" rx="3" />
      <path d="M5 15h30M9 12h4M15 12h4M21 12h4M11 21l4 3-4 3M18 28h6" />
    </svg>
  );
}

export function AccountDot(props: { id: AccountId }) {
  return <span class="dot" style={{ background: accountColor(props.id) }} />;
}

export function CheckIcon() {
  return <Stroke d="m3.5 8.5 3 3 6-7" size={12} width={2} />;
}

export function WarnIcon() {
  return (
    <Stroke
      d="M8 5.5v3.2M8 11h0M7.1 2.6 1.9 11.8c-.4.7.1 1.7 1 1.7h10.2c.9 0 1.4-1 1-1.7L8.9 2.6c-.4-.7-1.4-.7-1.8 0"
      size={14}
      width={1.5}
    />
  );
}

export function MenuIcon() {
  return <Stroke d="M3 4.5h10M3 8h10M3 11.5h10" size={16} />;
}
