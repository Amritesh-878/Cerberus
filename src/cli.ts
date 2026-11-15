import { invoke } from "@tauri-apps/api/core";

import type { Platform } from "./keys";

export interface Linked {
  message: string;
  /** The line to add to a shell startup file, when the link's folder isn't on `PATH`. */
  pathLine: string | null;
}

/** Windows' installer puts `cerberus` on `PATH`; macOS and Linux link it from Accounts. */
export function linksCli(platform: Platform): boolean {
  return platform !== "other";
}

export function installCli(): Promise<Linked> {
  return invoke<Linked>("install_cli");
}
