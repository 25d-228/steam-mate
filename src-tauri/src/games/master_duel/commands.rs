//! Tauri commands for Master Duel.
//!
//! Each command is an `async fn` returning `AppResult<T>`; the blocking work
//! (filesystem walks, junction syscalls, process enumeration) runs inside
//! `spawn_blocking` so the async runtime isn't stalled. Paths are derived from
//! [`paths::find_install`] each call — cheap, and keeps state out of the
//! command layer. A join failure of the blocking task maps to [`AppError::Io`].

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::games::master_duel::account::{MdAccount, SeedCandidate};
use crate::games::master_duel::{csv, link, paths};

/// Build the `MdAccount` list for the current install (blocking).
///
/// Scans each subdirectory of `LocalData` (skipping the shared `DATA` folder),
/// joins names from `accounts.csv` (missing name ⇒ empty string), and reports
/// each account's link state from its `<folder>/0000` junction.
fn list_accounts_blocking() -> AppResult<Vec<MdAccount>> {
    let install = paths::find_install()?;
    let local_data = paths::local_data(&install);
    let names = csv::read_accounts(&paths::accounts_csv(&install))?;

    let mut accounts = Vec::new();
    if local_data.exists() {
        for entry in std::fs::read_dir(&local_data)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let folder_id = entry.file_name().to_string_lossy().into_owned();
            // The shared cache lives under DATA, not a real account.
            if folder_id == "DATA" {
                continue;
            }
            let (account_name, steam_login) = names
                .iter()
                .find(|(id, _, _)| id == &folder_id)
                .map(|(_, n, l)| (n.clone(), l.clone()))
                .unwrap_or_default();
            // A genuine IO error probing one account's `0000` (e.g. the folder
            // is locked) must not abort the whole listing — degrade that one
            // row to "not linked" rather than failing every account.
            let is_linked = link::is_linked(&entry.path().join("0000")).unwrap_or(false);
            // Only probe for files when not linked — walking through a
            // junction would count the shared cache's files as the profile's.
            let has_files = !is_linked && link::has_files(&entry.path().join("0000"));
            accounts.push(MdAccount {
                folder_id,
                account_name,
                steam_login,
                is_linked,
                has_files,
            });
        }
    }
    Ok(accounts)
}

/// List all Master Duel accounts with their labels and link state.
#[tauri::command]
pub async fn md_list_accounts() -> AppResult<Vec<MdAccount>> {
    tauri::async_runtime::spawn_blocking(list_accounts_blocking)
        .await
        .map_err(|e| AppError::Io(e.to_string()))?
}

/// Link an account's `0000` folder to the shared cache via a junction.
///
/// `force` (default `false`) permits replacing a non-empty `0000` folder.
/// Refuses while the game is running.
#[tauri::command]
pub async fn md_link_account(folder_id: String, force: Option<bool>) -> AppResult<()> {
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let install = paths::find_install()?;
        let shared = paths::shared_cache(&install);
        let acct_0000 = paths::local_data(&install).join(&folder_id).join("0000");
        let running = link::is_running();
        link::link_account(&shared, &acct_0000, force, running)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Unlink an account's `0000` junction and rebuild an empty real folder.
///
/// Refuses while the game is running.
#[tauri::command]
pub async fn md_unlink_account(folder_id: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let install = paths::find_install()?;
        let acct_0000 = paths::local_data(&install).join(&folder_id).join("0000");
        let running = link::is_running();
        link::unlink_account(&acct_0000, running)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Link every currently-unlinked account to the shared cache.
///
/// Returns `{ linked, skipped }`: how many junctions were created and how many
/// accounts were left as-is. Safety contract:
/// - the running check happens **once** up front; if the game is running the
///   whole operation refuses ([`AppError::GameRunning`]) before any mutation,
///   so it can never partially apply while masterduel.exe is open;
/// - already-linked accounts are not touched (and not counted);
/// - `force = false`, so an account whose `0000` still holds its own files is
///   *skipped* (counted in `skipped`) rather than having that folder deleted;
/// - a per-account error never aborts the loop — that account is counted as
///   skipped and the rest proceed. Linking goes through [`link::link_account`],
///   which only ever creates a junction over an empty/removed real folder and
///   never deletes through a junction.
#[tauri::command]
pub async fn md_link_all() -> AppResult<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(|| {
        // Single up-front running check — never re-checked per account.
        if link::is_running() {
            return Err(AppError::GameRunning("master_duel"));
        }
        let install = paths::find_install()?;
        let shared = paths::shared_cache(&install);
        let local_data = paths::local_data(&install);

        let mut linked = 0u32;
        let mut skipped = 0u32;
        for account in list_accounts_blocking()? {
            if account.is_linked {
                continue; // already a junction — leave untouched, don't count
            }
            let acct_0000 = local_data.join(&account.folder_id).join("0000");
            // running = false: verified once above; force = false: never wipe
            // a folder that still has its own files.
            match link::link_account(&shared, &acct_0000, false, false) {
                Ok(()) => linked += 1,
                Err(_) => skipped += 1, // e.g. holds files — leave it, keep going
            }
        }
        Ok(serde_json::json!({ "linked": linked, "skipped": skipped }))
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Unlink every currently-linked account, rebuilding each as a real folder.
///
/// Returns the number of accounts unlinked. Safety contract:
/// - the running check happens **once** up front; if the game is running the
///   whole operation refuses ([`AppError::GameRunning`]) before any mutation;
/// - only currently-linked accounts are processed;
/// - a per-account error never aborts the loop — that account is left linked
///   and the rest proceed. Unlinking goes through [`link::unlink_account`],
///   which removes the junction with `remove_dir` (never `remove_dir_all`), so
///   the shared cache behind the link is never followed or deleted.
#[tauri::command]
pub async fn md_unlink_all() -> AppResult<u32> {
    tauri::async_runtime::spawn_blocking(|| {
        // Single up-front running check — never re-checked per account.
        if link::is_running() {
            return Err(AppError::GameRunning("master_duel"));
        }
        let install = paths::find_install()?;
        let local_data = paths::local_data(&install);

        let mut unlinked = 0u32;
        for account in list_accounts_blocking()? {
            if !account.is_linked {
                continue;
            }
            let acct_0000 = local_data.join(&account.folder_id).join("0000");
            // running = false: verified once above.
            if link::unlink_account(&acct_0000, false).is_ok() {
                unlinked += 1;
            }
        }
        Ok(unlinked)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Save (insert/update) the user-chosen label for an account in `accounts.csv`.
///
/// Touches only the `account_name` column — an existing `steam_login` is
/// preserved (see [`csv::upsert_account`]), so renaming never drops the
/// Steam assignment.
#[tauri::command]
pub async fn md_save_metadata(folder_id: String, account_name: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let install = paths::find_install()?;
        csv::upsert_account(&paths::accounts_csv(&install), &folder_id, &account_name)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Assign (or clear, with an empty string) the Steam login an account belongs to.
///
/// Touches only the `steam_login` column — the `account_name` label is
/// preserved (see [`csv::set_login`]). Pure metadata; never touches junctions,
/// so it is allowed while the game is running.
#[tauri::command]
pub async fn md_assign_steam(folder_id: String, steam_login: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let install = paths::find_install()?;
        csv::set_login(&paths::accounts_csv(&install), &folder_id, &steam_login)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Permanently delete an account (folder, saves) and drop its CSV row.
///
/// Always deletes saves. Refuses while the game is running.
#[tauri::command]
pub async fn md_delete_account(folder_id: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let install = paths::find_install()?;
        let local_data = paths::local_data(&install);
        let local_save = paths::local_save(&install);
        let running = link::is_running();
        link::delete_account(&local_data, &local_save, &folder_id, running)?;
        csv::remove_account(&paths::accounts_csv(&install), &folder_id)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Permanently delete several accounts in one pass — the batch behind the
/// UI's multi-select.
///
/// One up-front running check and one install resolve for the whole batch
/// (the same trade [`md_link_all`] makes), then the exact per-account flow of
/// [`md_delete_account`]. A profile that fails (locked folder, already gone)
/// is skipped and reported in `failed` rather than aborting the rest — a
/// batch must never strand itself half-done over one bad folder. Returns
/// `{ "deleted": n, "failed": [folder_id, ...] }`.
#[tauri::command]
pub async fn md_delete_accounts(folder_ids: Vec<String>) -> AppResult<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        // Single up-front running check — never re-checked per account.
        if link::is_running() {
            return Err(AppError::GameRunning("master_duel"));
        }
        let install = paths::find_install()?;
        let local_data = paths::local_data(&install);
        let local_save = paths::local_save(&install);
        let csv_path = paths::accounts_csv(&install);

        let mut deleted = 0u32;
        let mut failed: Vec<String> = Vec::new();
        for folder_id in &folder_ids {
            // running = false: verified once above (matching md_link_all); a
            // per-iteration re-probe would still race a mid-loop launch.
            let result = link::delete_account(&local_data, &local_save, folder_id, false)
                .and_then(|()| csv::remove_account(&csv_path, folder_id));
            match result {
                Ok(()) => deleted += 1,
                Err(_) => failed.push(folder_id.clone()),
            }
        }
        Ok(serde_json::json!({ "deleted": deleted, "failed": failed }))
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Whether `masterduel.exe` is currently running.
#[tauri::command]
pub async fn md_is_running() -> AppResult<bool> {
    tauri::async_runtime::spawn_blocking(link::is_running)
        .await
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Export the account list as pretty-printed JSON.
#[tauri::command]
pub async fn md_export_accounts() -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(|| {
        let accounts = list_accounts_blocking()?;
        serde_json::to_string_pretty(&accounts).map_err(|e| AppError::Io(e.to_string()))
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Export the account list as pretty-printed JSON to `path`.
#[tauri::command]
pub async fn md_export_to_file(path: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let accounts = list_accounts_blocking()?;
        let json = serde_json::to_string_pretty(&accounts)
            .map_err(|e| AppError::Io(e.to_string()))?;
        std::fs::write(Path::new(&path), json)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// The Master Duel install directory, located across all Steam libraries.
///
/// Returns the real path containing `masterduel.exe` (which may be in a
/// secondary library, e.g. `D:\SteamLibrary\...`), so the UI never shows a
/// primary-root path that doesn't exist. Errors with
/// [`AppError::GameNotInstalled`] if the game isn't found.
#[tauri::command]
pub async fn md_install_path() -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(paths::find_install()?.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Total byte size of the shared asset cache (0 if absent).
#[tauri::command]
pub async fn md_cache_size() -> AppResult<u64> {
    tauri::async_runtime::spawn_blocking(|| {
        let install = paths::find_install()?;
        Ok(link::cache_size(&paths::shared_cache(&install)))
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Whether the shared cache directory (`LocalData\DATA\0000`) exists.
///
/// Drives the Master Duel page's empty-state: `false` shows the "No shared
/// cache yet" box with the create flow, `true` shows the normal cache box.
#[tauri::command]
pub async fn md_cache_exists() -> AppResult<bool> {
    tauri::async_runtime::spawn_blocking(|| {
        let install = paths::find_install()?;
        Ok(paths::shared_cache(&install).exists())
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// List profiles that could seed a brand-new shared cache, largest first.
///
/// A candidate is a profile whose own `0000` is a REAL directory (not a
/// junction — already-linked profiles point at a cache, so there's nothing to
/// move) that holds at least one file (empty folders carry no cache to seed
/// from). Each candidate's `account_name` comes from `accounts.csv` (empty when
/// unlabeled) and its `size_bytes` is the `0000` folder's total file size (via
/// the same walk as [`link::cache_size`]). Sorted by `size_bytes` descending so
/// the most complete cache is offered first. The shared `DATA` folder is never
/// a candidate.
#[tauri::command]
pub async fn md_seed_candidates() -> AppResult<Vec<SeedCandidate>> {
    tauri::async_runtime::spawn_blocking(|| {
        let install = paths::find_install()?;
        let names = csv::read_accounts(&paths::accounts_csv(&install))?;
        // link::seed_candidates does the junction/size scan (already sorted
        // largest-first); we only join each id to its CSV label here.
        let candidates = link::seed_candidates(&paths::local_data(&install))?
            .into_iter()
            .map(|(folder_id, size_bytes)| {
                let account_name = names
                    .iter()
                    .find(|(id, _, _)| id == &folder_id)
                    .map(|(_, n, _)| n.clone())
                    .unwrap_or_default();
                SeedCandidate {
                    folder_id,
                    account_name,
                    size_bytes,
                }
            })
            .collect();
        Ok(candidates)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Create the shared cache, optionally seeding it from an existing profile.
///
/// `seed = Some(folder_id)`: that profile's own `0000` is *moved* (instant
/// same-volume rename, nothing copied) into `LocalData\DATA\0000` and a junction
/// is created back in its place — the profile becomes the first linked account.
/// `seed = None`: an empty skeleton (root + 256 hash folders) is laid down; the
/// game downloads the cache into it once on its next launch. Refuses while the
/// game is running and refuses if `DATA\0000` already exists — both enforced in
/// [`link::create_cache`].
#[tauri::command]
pub async fn md_create_cache(seed: Option<String>) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let install = paths::find_install()?;
        let shared = paths::shared_cache(&install);
        let running = link::is_running();
        match seed {
            Some(folder_id) => {
                let seed_0000 = paths::local_data(&install).join(&folder_id).join("0000");
                link::create_cache(&shared, Some(&seed_0000), running)
            }
            None => link::create_cache(&shared, None, running),
        }
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}

/// Open File Explorer at the shared cache path (`LocalData\DATA\0000`).
///
/// Errors with [`AppError::GameNotInstalled`] if the cache path is missing
/// (nothing to reveal). Launches `explorer.exe <path>` and returns immediately;
/// a spawn failure maps to [`AppError::Io`]. No extra plugin/permission needed —
/// `explorer` is invoked directly.
#[tauri::command]
pub async fn md_reveal_cache() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(|| {
        let install = paths::find_install()?;
        let shared = paths::shared_cache(&install);
        if !shared.exists() {
            return Err(AppError::GameNotInstalled("master_duel"));
        }
        std::process::Command::new("explorer")
            .arg(&shared)
            .spawn()
            .map_err(|e| AppError::Io(e.to_string()))?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))?
}
