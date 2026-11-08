import { invoke } from "@tauri-apps/api/core";

import type { AccountId } from "./accounts";

export type Changes =
  | { kind: "noGit" }
  | { kind: "notRepo" }
  | {
      kind: "repo";
      /** The repo folder's name. */
      name: string;
      /** The repo's top folder, so folders in one repo show it once. */
      root: string;
      /** `null` when HEAD is detached, and `commit` names it. */
      branch: string | null;
      commit: string | null;
      /** What the counts are against, such as `origin/main`; `null` counts uncommitted work only. */
      base: string | null;
      added: number;
      removed: number;
      /** Whether `gh` is installed for Create PR. */
      gh: boolean;
      /** Whether VS Code was found for Open in VS Code. */
      vscode: boolean;
    };

export interface PrRow {
  number: number;
  title: string;
  state: "draft" | "open" | "merged" | "closed";
  /** `owner/repo#n`, which a dismissal is saved under. */
  key: string;
  /** Changes whenever the PR does, so a dismissed one comes back. */
  updated: string;
}

export interface Connector {
  name: string;
  kind: "stdio" | "http" | "sse" | "ws" | "other";
  /** `app` is Cerberus's own server for messages between accounts, and plugin servers are named `<plugin>:<server>`. */
  scope: "plugin" | "user" | "project" | "local" | "app";
}

export interface Connectors {
  servers: Connector[];
  /** Config files that exist but could not be read. */
  unreadable: string[];
}

export interface Skill {
  name: string;
  description: string | null;
  /** `synced` comes from claude.ai, and plugin and synced skills are named `<plugin>:<skill>`. */
  scope: "project" | "account" | "synced" | "plugin";
  /** How often the account has used it; the core lists the most used first. */
  uses: number;
}

export function panelChanges(folder: string): Promise<Changes> {
  return invoke<Changes>("panel_changes", { folder });
}

/** Every PR of the folder's repo, newest first; the core asks GitHub at most once a minute per repo. */
export function panelPrs(folder: string): Promise<PrRow[]> {
  return invoke<PrRow[]>("panel_prs", { folder });
}

/** The core opens the link `gh` gave, so the webview never names a URL to open. */
export function openPr(folder: string, number: number): Promise<void> {
  return invoke("panel_open_pr", { folder, number });
}

export function prLink(folder: string, number: number): Promise<string> {
  return invoke<string>("panel_pr_link", { folder, number });
}

/** Runs `gh pr ready` for a draft; call only after the user confirmed. */
export function prReady(folder: string, number: number): Promise<void> {
  return invoke("panel_pr_ready", { folder, number });
}

export function openFolder(folder: string): Promise<void> {
  return invoke("panel_open_folder", { folder });
}

export function openVsCode(folder: string): Promise<void> {
  return invoke("panel_open_vscode", { folder });
}

/** `tab` lets the core say whether that tab's running chat was given Cerberus's own server. */
export function panelConnectors(
  account: AccountId | null,
  folder: string | null,
  tab: number,
): Promise<Connectors> {
  return invoke<Connectors>("panel_connectors", { account, folder, tab });
}

export function panelSkills(
  account: AccountId | null,
  folder: string | null,
): Promise<Skill[]> {
  return invoke<Skill[]>("panel_skills", { account, folder });
}

/** How a connector's origin is shown, with Cerberus's own server under the app's name. */
export function connectorScope(scope: Connector["scope"]): string {
  return scope === "app" ? "Cerberus" : scope;
}

/** The mark a skill row shows for where it comes from, with none for the project's and the account's own. */
export function skillSource(scope: Skill["scope"]): string | null {
  if (scope === "plugin") return "plugin";
  if (scope === "synced") return "claude.ai";
  return null;
}

/** The command Create PR types into a new shell tab, for the user to send. */
export const CREATE_PR = "gh pr create --fill";

/** Starred skills first, each group kept in the core's use order. */
export function favouritesFirst(
  skills: readonly Skill[],
  favourites: readonly string[],
): Skill[] {
  const starred = skills.filter((s) => favourites.includes(s.name));
  return [...starred, ...skills.filter((s) => !favourites.includes(s.name))];
}

/** What a shift-click types into a chat: control characters dropped, so a name can never carry an Enter. */
export function skillCommand(name: string): string {
  return `/${name.replace(/\p{Cc}/gu, "")}`;
}
