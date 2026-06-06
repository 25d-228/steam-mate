import { invoke } from "@tauri-apps/api/core";
import type { MdAccount } from "../types";

export function listAccounts(): Promise<MdAccount[]> {
  return invoke<MdAccount[]>("md_list_accounts");
}

export function linkAccount(folderId: string, force?: boolean): Promise<void> {
  return invoke<void>("md_link_account", { folderId, force });
}

export function unlinkAccount(folderId: string): Promise<void> {
  return invoke<void>("md_unlink_account", { folderId });
}

export function saveMetadata(
  folderId: string,
  accountName: string,
): Promise<void> {
  return invoke<void>("md_save_metadata", { folderId, accountName });
}

export function deleteAccount(folderId: string): Promise<void> {
  return invoke<void>("md_delete_account", { folderId });
}

/** Assign (or clear, with an empty string) the Steam login a profile belongs to. */
export function assignSteam(folderId: string, steamLogin: string): Promise<void> {
  return invoke<void>("md_assign_steam", { folderId, steamLogin });
}

/** Link every unlinked account. Returns how many were linked and how many skipped. */
export function linkAll(): Promise<{ linked: number; skipped: number }> {
  return invoke<{ linked: number; skipped: number }>("md_link_all");
}

/** Unlink every linked account. Returns how many were unlinked. */
export function unlinkAll(): Promise<number> {
  return invoke<number>("md_unlink_all");
}

export function isRunning(): Promise<boolean> {
  return invoke<boolean>("md_is_running");
}

export function exportAccounts(): Promise<string> {
  return invoke<string>("md_export_accounts");
}

export function exportToFile(path: string): Promise<void> {
  return invoke<void>("md_export_to_file", { path });
}

export function cacheSize(): Promise<number> {
  return invoke<number>("md_cache_size");
}

export function installPath(): Promise<string> {
  return invoke<string>("md_install_path");
}
