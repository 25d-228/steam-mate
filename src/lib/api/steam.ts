import { invoke } from "@tauri-apps/api/core";
import type { SteamAccount } from "./types";

export function getInstallPath(): Promise<string> {
  return invoke<string>("steam_get_install_path");
}

export function listAccounts(): Promise<SteamAccount[]> {
  return invoke<SteamAccount[]>("steam_list_accounts");
}

export function clearLogin(): Promise<void> {
  return invoke<void>("steam_clear_login");
}
