import { invoke } from "@tauri-apps/api/core";
import type { SteamAccount, GameInfo } from "./types";

export function getInstallPath(): Promise<string> {
  return invoke<string>("steam_get_install_path");
}

export function listAccounts(): Promise<SteamAccount[]> {
  return invoke<SteamAccount[]>("steam_list_accounts");
}

export function clearLogin(): Promise<void> {
  return invoke<void>("steam_clear_login");
}

export function switchAccount(
  accountName: string,
  offlineMode?: boolean,
): Promise<void> {
  return invoke<void>("steam_switch_account", { accountName, offlineMode });
}

export function forgetAccount(accountName: string): Promise<void> {
  return invoke<void>("steam_forget_account", { accountName });
}

/** Forget several remembered accounts at once; resolves with how many were removed. */
export function forgetAccounts(accountNames: string[]): Promise<number> {
  return invoke<number>("steam_forget_accounts", { accountNames });
}

export function getAvatar(steamId64: string): Promise<string | null> {
  return invoke<string | null>("steam_get_avatar", { steamId64 });
}

export function isRunning(): Promise<boolean> {
  return invoke<boolean>("steam_is_running");
}

export function listSupportedGames(): Promise<GameInfo[]> {
  return invoke<GameInfo[]>("list_supported_games");
}
