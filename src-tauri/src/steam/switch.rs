//! Account-switch orchestrator: kill -> rewrite registry+loginusers -> relaunch.
//! Wraps the per-step modules into a fixed sequence. These functions are
//! synchronous; the Tauri commands run them via `spawn_blocking`.

use std::fs;
use std::io::Write;
use std::path::Path;

use super::{paths, process, registry, vdf};
use crate::error::{AppError, AppResult};

/// Write `text` to `path` atomically: write a sibling temp file, flush it to
/// stable storage, then rename it over the target.
///
/// `fs::rename` is atomic on a single volume, so a crash mid-write can't leave
/// `loginusers.vdf` truncated — readers see either the old file or the new one.
/// We `sync_all` the temp file before renaming so that even an OS/power-loss
/// crash (not just a process crash) can't leave the renamed file with
/// unflushed, zero-length contents.
fn atomic_write(path: &Path, text: &str) -> AppResult<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".steam-mate.tmp");
    let tmp = std::path::PathBuf::from(tmp);

    let mut file = fs::File::create(&tmp).map_err(|e| AppError::Io(e.to_string()))?;
    file.write_all(text.as_bytes())
        .map_err(|e| AppError::Io(e.to_string()))?;
    // Flush the data to disk before the rename so the rename can't publish an
    // empty/partial file after a power loss.
    file.sync_all().map_err(|e| AppError::Io(e.to_string()))?;
    drop(file);

    fs::rename(&tmp, path).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

/// Rewrite Steam `config.vdf` text so `AlwaysShowUserChooser` is `"0"`.
///
/// When that key is `"1"`, Steam ignores the auto-login user and always stops at
/// the account chooser on launch — so the account we switched to never signs in
/// silently, no matter how correct the registry and `loginusers.vdf` are. We
/// flip just that one value and leave the rest of the (large) file byte-for-byte
/// intact. Returns `Some(new_text)` only when a change is actually needed;
/// `None` if the key is already `"0"` or absent (absent is Steam's default
/// auto-login behavior, which is what we want anyway).
fn set_chooser_off(config_text: &str) -> Option<String> {
    let key = "\"AlwaysShowUserChooser\"";
    let key_idx = config_text.find(key)?;
    let after_key = key_idx + key.len();
    // The first quote after the key opens the value; the next quote closes it.
    let val_start = after_key + config_text[after_key..].find('"')? + 1;
    let val_end = val_start + config_text[val_start..].find('"')?;
    if &config_text[val_start..val_end] == "0" {
        return None;
    }
    let mut out = String::with_capacity(config_text.len());
    out.push_str(&config_text[..val_start]);
    out.push('0');
    out.push_str(&config_text[val_end..]);
    Some(out)
}

/// Ensure Steam silently signs into the switched account instead of stopping at
/// the account chooser.
///
/// Sets `AlwaysShowUserChooser` to `"0"` in `config.vdf` when it isn't already.
/// Best-effort by design: a missing, unreadable, or unwritable `config.vdf` (or
/// one without the key) is not an error — a switch must never fail over it.
/// Steam must be stopped first, since a running client rewrites `config.vdf` on
/// exit and would undo the change.
fn force_auto_login() {
    let Ok(path) = paths::config_vdf_path() else {
        return;
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    if let Some(new_text) = set_chooser_off(&text) {
        let _ = atomic_write(&path, &new_text);
    }
}

/// Switch Steam's active account to `account_name`, optionally launching offline.
///
/// Steps, strictly in order:
pub fn switch_account(account_name: &str, offline_mode: bool) -> AppResult<()> {
    // 1. Locate and read loginusers.vdf; a missing file means Steam isn't installed.
    let vdf_path = paths::loginusers_vdf_path()?;
    let text = fs::read_to_string(&vdf_path).map_err(|_| AppError::SteamNotInstalled)?;

    // 2. Compute the new VDF text FIRST so an unknown account fails fast — before
    //    we kill Steam or touch the registry, leaving the system untouched.
    let new_text = vdf::set_active_account(&text, account_name, offline_mode)?;

    // 3. Stop Steam: it caches loginusers.vdf in memory and rewrites it on exit,
    //    so it must be down before we change the file or the registry.
    let install = paths::get_steam_install_path()?;
    let steam_exe = install.join("Steam.exe");
    process::kill_steam(&steam_exe)?;

    // 3b. With Steam stopped, make sure it will auto-login the target rather than
    //     stop at the account chooser (the `AlwaysShowUserChooser=1` trap, which
    //     otherwise blocks silent sign-in for every account). Best-effort.
    force_auto_login();

    // 4. + 5. Write the registry auto-login user and loginusers.vdf. Steam is now
    //    force-closed, so if either write fails we must still relaunch Steam
    //    before returning the error — otherwise the user is left with Steam
    //    killed and never restarted.
    let write_result = registry::set_auto_login_user(account_name)
        .and_then(|()| atomic_write(&vdf_path, &new_text));

    // 6. Relaunch Steam (it auto-signs into the chosen account on success). On a
    //    write failure, surface that error but still try to bring Steam back up.
    match write_result {
        Ok(()) => process::start_steam(&steam_exe),
        Err(write_err) => {
            let _ = process::start_steam(&steam_exe);
            Err(write_err)
        }
    }
}

/// Forget (delete) a remembered account from `loginusers.vdf`.
///
/// Reads the file, drops the matching block via [`vdf::remove_account`], and
/// writes the result back atomically.
///
/// Steam is stopped first: a running Steam holds its own copy of
/// `loginusers.vdf` and rewrites the file on exit, which would resurrect the
/// entry we just removed and make the UI's "forgotten" message false. Stopping
/// it (like [`switch_account`] does) makes the removal stick.
pub fn forget_account(account_name: &str) -> AppResult<()> {
    let vdf_path = paths::loginusers_vdf_path()?;
    let text = fs::read_to_string(&vdf_path).map_err(|_| AppError::SteamNotInstalled)?;
    // Compute the new text first so an unknown account fails fast — before we
    // kill Steam, leaving the system untouched.
    let new_text = vdf::remove_account(&text, account_name)?;

    // Stop Steam so it can't rewrite loginusers.vdf on exit and resurrect the
    // entry. No-op if Steam isn't running.
    let install = paths::get_steam_install_path()?;
    process::kill_steam(&install.join("Steam.exe"))?;

    atomic_write(&vdf_path, &new_text)?;
    Ok(())
}

/// Forget several remembered accounts in one pass, returning how many were
/// actually removed.
///
/// Unlike calling [`forget_account`] per name, this kills Steam once, reads
/// `loginusers.vdf` once, applies [`vdf::remove_account`] successively over the
/// in-memory text for each name, and writes once. A name that isn't present
/// ([`AppError::AccountNotFound`]) is skipped rather than aborting the batch —
/// only genuine parse failures propagate — so the count reflects the accounts
/// that existed and were dropped. If nothing matched, the file is left untouched
/// (no needless rewrite) and `0` is returned, but Steam is still stopped first
/// (the caller asked to forget, and a no-op write is cheap to skip).
///
/// After a successful write, if the registry's remembered auto-login user was
/// among the removed names, the auto-login keys are cleared so Steam doesn't try
/// to sign straight into an account that no longer has a saved login. A failure
/// to clear is swallowed — the accounts are already gone, which is the contract.
pub fn forget_accounts(account_names: &[String]) -> AppResult<u32> {
    // Stop Steam BEFORE reading: Steam rewrites loginusers.vdf while running
    // and on exit, so a read taken first could capture text Steam is about to
    // supersede — our later write would then silently clobber Steam's (the
    // single-account forget kills first for the same reason). No-op if Steam
    // isn't running.
    let install = paths::get_steam_install_path()?;
    process::kill_steam(&install.join("Steam.exe"))?;

    let vdf_path = paths::loginusers_vdf_path()?;
    let text = fs::read_to_string(&vdf_path).map_err(|_| AppError::SteamNotInstalled)?;

    // Apply each removal to the running text. Skip names that aren't present;
    // surface only real parse failures.
    let mut current = text;
    let mut removed: Vec<&str> = Vec::new();
    for name in account_names {
        match vdf::remove_account(&current, name) {
            Ok(next) => {
                current = next;
                removed.push(name.as_str());
            }
            Err(AppError::AccountNotFound(_)) => continue,
            Err(e) => return Err(e),
        }
    }

    if removed.is_empty() {
        return Ok(0);
    }

    atomic_write(&vdf_path, &current)?;

    // If the auto-login user was one of the removed accounts, clear the
    // registry keys so Steam lands on the login screen instead of an orphan.
    if let Some(active) = registry::get_auto_login_user() {
        if removed.iter().any(|name| *name == active) {
            let _ = registry::clear_auto_login_user();
        }
    }

    Ok(removed.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::set_chooser_off;

    #[test]
    fn flips_chooser_from_one_to_zero() {
        // Real Steam writes the value tab-separated after the key.
        let cfg = "\t\t\t\t\"AlwaysShowUserChooser\"\t\t\"1\"\n";
        let out = set_chooser_off(cfg).expect("a change is needed");
        assert!(out.contains("\"AlwaysShowUserChooser\"\t\t\"0\""));
        assert!(!out.contains("\"1\""));
    }

    #[test]
    fn no_change_when_already_zero() {
        let cfg = "\"AlwaysShowUserChooser\"\t\t\"0\"";
        assert!(set_chooser_off(cfg).is_none());
    }

    #[test]
    fn no_change_when_key_absent() {
        // Absent key = Steam's default auto-login behavior; nothing to do.
        let cfg = "\"SomethingElse\"\t\t\"1\"";
        assert!(set_chooser_off(cfg).is_none());
    }

    #[test]
    fn preserves_surrounding_content() {
        let cfg = "{ \"a\" \"keep\" \"AlwaysShowUserChooser\" \"1\" \"b\" \"keep2\" }";
        let out = set_chooser_off(cfg).unwrap();
        assert!(out.contains("\"a\" \"keep\""));
        assert!(out.contains("\"b\" \"keep2\""));
        assert!(out.contains("\"AlwaysShowUserChooser\" \"0\""));
    }
}
