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
  /** `null` starts tabs in the user's home folder. */
  startFolder: string | null;
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

export function removeAccount(id: AccountId): Promise<Profiles> {
  return invoke<Profiles>("remove_account", { id });
}

export function setDefaultAccount(id: AccountId): Promise<Profiles> {
  return invoke<Profiles>("set_default_account", { id });
}

export function setStartFolder(folder: string | null): Promise<Profiles> {
  return invoke<Profiles>("set_start_folder", { folder });
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
