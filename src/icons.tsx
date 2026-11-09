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

export function AccountDot(props: {
  id: AccountId;
  color: string | undefined;
}) {
  return (
    <span
      class="dot"
      style={{ background: accountColor(props.id, props.color) }}
    />
  );
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

export function PanelIcon() {
  return (
    <Stroke
      d="M3.5 3h9A1.5 1.5 0 0 1 14 4.5v7a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5v-7A1.5 1.5 0 0 1 3.5 3M10 3v10"
      size={16}
      width={1.4}
    />
  );
}

/** The panel icon mirrored: a window with its left column marked, for the sidebar. */
export function SidebarIcon() {
  return (
    <Stroke
      d="M3.5 3h9A1.5 1.5 0 0 1 14 4.5v7a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5v-7A1.5 1.5 0 0 1 3.5 3M6 3v10"
      size={16}
      width={1.4}
    />
  );
}

export function BranchIcon() {
  return (
    <Stroke
      d="M5 3v10M5 13a1.5 1.5 0 1 0 0-.01M11 6.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3M11 6.5c0 2.5-6 2-6 5"
      width={1.4}
    />
  );
}

export function DiffIcon() {
  return (
    <Stroke
      d="M3.5 2.5h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1M8 4.8v4.4M5.8 7h4.4M5.8 11.2h4.4"
      width={1.3}
    />
  );
}

export function PrIcon() {
  return (
    <Stroke
      d="M4.5 5.5v8M4.5 5.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3M11.5 10.5V6.5c0-1.1-.9-2-2-2H7.5M9 3l-1.5 1.5L9 6M11.5 13.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3"
      width={1.4}
    />
  );
}

export function PlugIcon() {
  return (
    <Stroke
      d="M6 2v3M10 2v3M4.5 5h7v2.5a3.5 3.5 0 0 1-7 0zM8 11v3"
      width={1.4}
    />
  );
}

export function SkillIcon() {
  return (
    <Stroke
      d="M8 2.5 9.4 6.6 13.5 8 9.4 9.4 8 13.5 6.6 9.4 2.5 8 6.6 6.6z"
      width={1.3}
    />
  );
}

export function StarIcon() {
  return (
    <Stroke
      d="M8 2.2 9.8 5.9l4 .6-2.9 2.8.7 4L8 11.4l-3.6 1.9.7-4-2.9-2.8 4-.6z"
      size={13}
      width={1.3}
    />
  );
}

export function CodeIcon() {
  return <Stroke d="M5.5 4.5 2 8l3.5 3.5M10.5 4.5 14 8l-3.5 3.5" size={13} />;
}

export function GearIcon() {
  return (
    <Stroke
      d="M8 5.6a2.4 2.4 0 1 0 0 4.8 2.4 2.4 0 0 0 0-4.8M8 1.5v1.8M8 12.7v1.8M1.5 8h1.8M12.7 8h1.8M3.4 3.4l1.3 1.3M11.3 11.3l1.3 1.3M3.4 12.6l1.3-1.3M11.3 4.7l1.3-1.3"
      size={13}
      width={1.3}
    />
  );
}

export function MoreIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="3.5" cy="8" r="1.2" fill="currentColor" />
      <circle cx="8" cy="8" r="1.2" fill="currentColor" />
      <circle cx="12.5" cy="8" r="1.2" fill="currentColor" />
    </svg>
  );
}
