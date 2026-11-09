import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import palette from "./palette.json";
import type { Step } from "./steps";

export type AccountId = number;

export interface Account {
  id: AccountId;
  name: string;
  configDir: string;
  /** A palette name; absent keeps the colour the account's id gave it before colours were saved. */
  color?: string;
}

/** The right panel's cards that fold to their title. */
export type PanelCard =
  "environment" | "context" | "usage" | "changes" | "connectors" | "skills";

export interface PanelPrefs {
  open: boolean;
  /** In pixels; `null` keeps the default. */
  width: number | null;
  collapsed: PanelCard[];
  /** Cards folded at first that the user opened. */
  expanded: PanelCard[];
  /** Starred skill names by account id. */
  favourites: Partial<Record<string, string[]>>;
  /** Dismissed PRs by `owner/repo#n`, to the update time they were dismissed at; the core always sends it. */
  dismissed?: Partial<Record<string, string>>;
}

export interface Profiles {
  accounts: Account[];
  defaultAccount: AccountId | null;
  /** The program id the Shell button opens; `null` opens the first shell installed. */
  shell: string | null;
  /** Where New project's folder picker opens; `null` leaves it to Windows. */
  startFolder: string | null;
  /** In pixels; `null` keeps the default. */
  sidebarWidth: number | null;
  panel: PanelPrefs;
  /** The left chat's share of a split in thousandths; `null` keeps half and half. */
  splitShare: number | null;
  /** The sidebar is folded to its rail; a narrow window folds it without saving. */
  sidebarCollapsed: boolean;
  /** Desktop notifications when a chat in another tab needs you. */
  alerts: boolean;
  /** Shell tabs suggest from history as you type, through each shell's own feature. */
  suggestions: boolean;
  /** Claude tabs can message tabs on other accounts, each message approved first. */
  messages: boolean;
  /** Claude tabs report context and limits through their status line for the live cards. */
  live: boolean;
  /** The 5-hour use, in percent, at which an account's warning shows. */
  warnAt: number;
  /** The walkthrough step to reopen at; `null` once setup is finished or skipped. */
  setup: Step | null;
  /** Removed accounts, still named on the tabs left on them. */
  retired: Account[];
  revision: number;
}

/** `false` when `fresh` was saved before `current`, so a late reply cannot undo a newer change. */
export function isNewer(
  fresh: Profiles,
  current: Profiles | undefined,
): boolean {
  return current === undefined || fresh.revision >= current.revision;
}

export interface Candidate {
  name: string;
  configDir: string;
}

export interface AuthStatus {
  state: "loggedIn" | "loggedOut" | "unknown";
  summary: string;
  /** Set when signed in and reported; a check that failed in the webview has neither. */
  email?: string | null;
  plan?: string | null;
}

export function loadProfiles(): Promise<Profiles> {
  return invoke<Profiles>("load_profiles");
}

export function resetProfiles(): Promise<Profiles> {
  return invoke<Profiles>("reset_profiles");
}

export function detectAccounts(): Promise<Candidate[]> {
  return invoke<Candidate[]>("detect_accounts");
}

export function suggestConfigDir(name: string): Promise<string> {
  return invoke<string>("suggest_config_dir", { name });
}

export function addAccount(name: string, configDir: string): Promise<Profiles> {
  return invoke<Profiles>("add_account", { name, configDir });
}

export function renameAccount(id: AccountId, name: string): Promise<Profiles> {
  return invoke<Profiles>("rename_account", { id, name });
}

/** `inUse` changes nothing: it names the saved tabs on the account, so the user can be asked first. */
export type Removal =
  { kind: "inUse"; tabs: string[] } | { kind: "removed"; profiles: Profiles };

export function removeAccount(
  id: AccountId,
  confirmed: boolean,
): Promise<Removal> {
  return invoke<Removal>("remove_account", { id, confirmed });
}

/** A removed account's tabs stay, but nothing may run on it until the chat moves. */
export function isRemoved(profiles: Profiles, id: AccountId): boolean {
  return !profiles.accounts.some((a) => a.id === id);
}

/** The line a tab's menu shows beside its account. */
export function accountState(
  profiles: Profiles,
  status: AuthStatus | undefined,
  id: AccountId,
): string {
  return isRemoved(profiles, id) ? "removed" : statusText(status);
}

export function setDefaultAccount(id: AccountId): Promise<Profiles> {
  return invoke<Profiles>("set_default_account", { id });
}

export function setStartFolder(folder: string | null): Promise<Profiles> {
  return invoke<Profiles>("set_start_folder", { folder });
}

export function setSidebarWidth(width: number): Promise<Profiles> {
  return invoke<Profiles>("set_sidebar_width", { width });
}

let collapseSaving: Promise<unknown> = Promise.resolve();

/** One save at a time, in click order, so a late reply cannot undo a newer choice. */
export function setSidebarCollapsed(collapsed: boolean): Promise<Profiles> {
  const saved = collapseSaving.then(() =>
    invoke<Profiles>("set_sidebar_collapsed", { collapsed }),
  );
  collapseSaving = saved.catch(() => undefined);
  return saved;
}

export function setSplitShare(share: number): Promise<Profiles> {
  return invoke<Profiles>("set_split_share", { share });
}

export function setPanel(panel: PanelPrefs): Promise<Profiles> {
  return invoke<Profiles>("set_panel", { panel });
}

// One save at a time for the whole app, so the last click stays even across reopening a screen.
let alertsSaving: Promise<unknown> = Promise.resolve();

export function setAlerts(on: boolean): Promise<Profiles> {
  const saved = alertsSaving.then(() => invoke<Profiles>("set_alerts", { on }));
  alertsSaving = saved.catch(() => undefined);
  return saved;
}

let suggestionsSaving: Promise<unknown> = Promise.resolve();

export function setSuggestions(on: boolean): Promise<Profiles> {
  const saved = suggestionsSaving.then(() =>
    invoke<Profiles>("set_suggestions", { on }),
  );
  suggestionsSaving = saved.catch(() => undefined);
  return saved;
}

let messagesSaving: Promise<unknown> = Promise.resolve();

export function setMessages(on: boolean): Promise<Profiles> {
  const saved = messagesSaving.then(() =>
    invoke<Profiles>("set_messages", { on }),
  );
  messagesSaving = saved.catch(() => undefined);
  return saved;
}

// One save at a time, in click order, so a step saved late cannot reopen setup after Skip or Finish.
let setupSaving: Promise<unknown> = Promise.resolve();

/** `null` ends setup, by Finish or Skip. */
export function setSetupStep(step: Step | null): Promise<Profiles> {
  const saved = setupSaving.then(() =>
    invoke<Profiles>("set_setup_step", { step }),
  );
  setupSaving = saved.catch(() => undefined);
  return saved;
}

export function setShell(shell: string): Promise<Profiles> {
  return invoke<Profiles>("set_shell", { shell });
}

/** Resolves to `null` when the user cancels. */
export function pickFolder(from?: string): Promise<string | null> {
  return open({ directory: true, defaultPath: from });
}

export function accountStatus(id: AccountId): Promise<AuthStatus> {
  return invoke<AuthStatus>("account_status", { id });
}

export function preselectedAccount(profiles: Profiles): AccountId | null {
  const saved = profiles.accounts.find((a) => a.id === profiles.defaultAccount);
  return (saved ?? profiles.accounts[0])?.id ?? null;
}

export interface Swatch {
  name: string;
  label: string;
  hex: string;
}

/** The colours an account can take; the core checks names against the same file. */
export const PALETTE: readonly Swatch[] = palette;

// Before colours were saved, an account's id picked one of the first five.
const LEGACY_COLOURS = 5;

/** The swatch an account shows: its saved one, else the one its id gave it. */
export function swatchOf(id: AccountId, color: string | undefined): Swatch {
  const saved = PALETTE.find((s) => s.name === color);
  return saved ?? PALETTE[id % LEGACY_COLOURS] ?? FALLBACK;
}

const FALLBACK: Swatch = { name: "grey", label: "Grey", hex: "#8a8a8a" };

export function accountColor(id: AccountId, color: string | undefined): string {
  return swatchOf(id, color).hex;
}

/** An account's saved colour by id, a removed one included, for dots on tabs that only know the id. */
export function savedColor(
  profiles: Profiles,
  id: AccountId,
): string | undefined {
  return [...profiles.accounts, ...profiles.retired].find((a) => a.id === id)
    ?.color;
}

export function setAccountColor(
  id: AccountId,
  color: string,
): Promise<Profiles> {
  return invoke<Profiles>("set_account_color", { id, color });
}

export function statusText(status: AuthStatus | undefined): string {
  return status?.summary ?? "checking…";
}
