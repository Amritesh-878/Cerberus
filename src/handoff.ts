import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type { Output } from "./live";
import type { HandoffState, TabHandoff, TabId } from "./tree";

/** Whether the chat reported writing the file, or it was written since the task opened. */
export function handoffWritten(
  tab: TabId,
  file: string,
  since: number,
): Promise<boolean> {
  return invoke<boolean>("handoff_written", { tab, file, since });
}

export interface HandoffMessage {
  /** The given id, or one the core read from the file's name. */
  taskId: string;
  text: string;
}

/** The line for the planner: task id, path and the branch's PR, never the file's content. */
export function handoffMessage(
  folder: string,
  file: string,
  taskId: string | null,
): Promise<HandoffMessage> {
  return invoke<HandoffMessage>("handoff_message", { folder, file, taskId });
}

/** Queues the line in the approval popup for a planner on another account. */
export function handoffOffer(
  from: TabId,
  to: TabId,
  text: string,
): Promise<number> {
  return invoke<number>("handoff_offer", { from, to, text });
}

const STATE_TEXT: Record<HandoffState, string> = {
  working: "working",
  ready: "handoff ready",
  sent: "handoff sent",
};

/** "TASK-026 · handoff ready", for the tab's row. */
export function handoffLine(handoff: TabHandoff): string {
  return `${handoff.taskId} · ${STATE_TEXT[handoff.state]}`;
}

/** The newest Markdown file the chat wrote, which Send handoff… offers first. */
export function newestMarkdown(outputs: readonly Output[]): string | undefined {
  return outputs.find((o) => o.path.toLowerCase().endsWith(".md"))?.path;
}

/** The handoff file, starting at `from`; `null` when the user cancels. */
export function pickHandoff(from: string | undefined): Promise<string | null> {
  return open({
    directory: false,
    multiple: false,
    defaultPath: from,
    filters: [{ name: "Markdown", extensions: ["md"] }],
  });
}
