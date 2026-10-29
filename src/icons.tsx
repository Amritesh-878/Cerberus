import { Show } from "solid-js";
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

export function PanelIcon() {
  return (
    <Stroke
      d="M3.5 3h9A1.5 1.5 0 0 1 14 4.5v7a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5v-7A1.5 1.5 0 0 1 3.5 3M10 3v10"
      size={16}
      width={1.4}
    />
  );
}

export function MenuIcon() {
  return <Stroke d="M3 4.5h10M3 8h10M3 11.5h10" size={16} />;
}
