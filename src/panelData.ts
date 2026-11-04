import { invoke } from "@tauri-apps/api/core";

import type { AccountId } from "./accounts";

export type Changes =
  | { kind: "noGit" }
  | { kind: "notRepo" }
  | {
      kind: "repo";
      /** The repo folder's name. */
      name: string;
      /** `null` when HEAD is detached, and `commit` names it. */
      branch: string | null;
      commit: string | null;
      /** What the counts are against, such as `origin/main`; `null` counts uncommitted work only. */
      base: string | null;
      added: number;
      removed: number;
      /** Whether `gh` is installed for Create PR. */
      gh: boolean;
    };

export interface PrRow {
  number: number;
  title: string;
}

export interface Connector {
  name: string;
  kind: "stdio" | "http" | "sse" | "ws" | "other";
  scope: "user" | "project" | "local";
}

export interface Connectors {
  servers: Connector[];
  /** Config files that exist but could not be read. */
  unreadable: string[];
}

export interface Skill {
  name: string;
  description: string | null;
  scope: "project" | "account";
}

export function panelChanges(folder: string): Promise<Changes> {
  return invoke<Changes>("panel_changes", { folder });
}

/** The branch's open PR; the core asks GitHub at most once a minute per repo. */
export function panelPr(folder: string): Promise<PrRow | null> {
  return invoke<PrRow | null>("panel_pr", { folder });
}

export function openPr(folder: string): Promise<void> {
  return invoke("panel_open_pr", { folder });
}

export function openFolder(folder: string): Promise<void> {
  return invoke("panel_open_folder", { folder });
}

export function panelConnectors(
  account: AccountId | null,
  folder: string | null,
): Promise<Connectors> {
  return invoke<Connectors>("panel_connectors", { account, folder });
}

export function panelSkills(
  account: AccountId | null,
  folder: string | null,
): Promise<Skill[]> {
  return invoke<Skill[]>("panel_skills", { account, folder });
}

/** The command Create PR types into a new shell tab, for the user to send. */
export const CREATE_PR = "gh pr create --fill";
