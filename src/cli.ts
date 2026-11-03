import { invoke } from "@tauri-apps/api/core";

export interface Linked {
  message: string;
  /** The line to add to a shell startup file, when the link's folder isn't on `PATH`. */
  pathLine: string | null;
}

export function installCli(): Promise<Linked> {
  return invoke<Linked>("install_cli");
}
