import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type AccountId = number;

export interface Account {
  id: AccountId;
  name: string;
  configDir: string;
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
  /** Desktop notifications when a chat in another tab needs you. */
  alerts: boolean;
  /** Shell tabs suggest from history as you type, through each shell's own feature. */
  suggestions: boolean;
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

const ACCOUNT_COLORS = ["#d97757", "#7aa2c8", "#7fb27f", "#b58ad6", "#d6b45a"];

export function accountColor(id: AccountId): string {
  return ACCOUNT_COLORS[id % ACCOUNT_COLORS.length] ?? "#8a8a8a";
}

export function statusText(status: AuthStatus | undefined): string {
  return status?.summary ?? "checking…";
}
