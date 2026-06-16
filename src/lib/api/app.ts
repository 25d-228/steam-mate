import { invoke } from "@tauri-apps/api/core";

/**
 * Quit the whole app — the tray popup's "Exit". Closing the main window only
 * hides it to the tray, so this is the only path that actually exits.
 */
export function appExit(): Promise<void> {
  return invoke<void>("app_exit");
}
