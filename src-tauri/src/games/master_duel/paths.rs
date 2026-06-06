//! Locate the Master Duel install dir + its data folders.
//!
//! Master Duel can live in any Steam library, so we read
//! `libraryfolders.vdf` to enumerate every library root and probe each for
//! the game. The install folder name has TWO spaces between `!` and `Master`:
//! `"Yu-Gi-Oh!  Master Duel"` — Konami's exact on-disk name; do not "fix" it.

use std::path::{Path, PathBuf};

use keyvalues_parser::{Value, Vdf};

use crate::error::{AppError, AppResult};

/// Konami's install folder name. Note the TWO spaces after `!`.
const INSTALL_DIR_NAME: &str = "Yu-Gi-Oh!  Master Duel";

/// Recursively collect every value of a key named `"path"` from a VDF tree.
///
/// `libraryfolders.vdf` nests each library under a numeric index with a
/// `"path"` field; walking generically tolerates schema drift between Steam
/// versions (older files were flat string maps, newer ones are nested objects).
fn collect_paths(value: &Value<'_>, out: &mut Vec<PathBuf>) {
    let Value::Obj(obj) = value else { return };
    for (key, values) in obj.0.iter() {
        for v in values {
            if key.as_ref() == "path" {
                if let Some(s) = v.get_str() {
                    out.push(PathBuf::from(s));
                }
            }
            collect_paths(v, out);
        }
    }
}

/// Enumerate every Steam library root: `steam_root` plus each `"path"` found
/// in `<steam>/steamapps/libraryfolders.vdf`. A missing/unparsable file is
/// tolerated — we just fall back to the root.
fn library_roots(steam_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![steam_root.to_path_buf()];
    let vdf_path = steam_root
        .join("steamapps")
        .join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf_path) {
        if let Ok(partial) = keyvalues_parser::parse(&text) {
            let vdf = Vdf::from(partial);
            collect_paths(&vdf.value, &mut roots);
        }
    }
    roots
}

/// Find the Master Duel install directory across all Steam libraries.
///
/// Returns the directory containing `masterduel.exe`, or
/// `AppError::GameNotInstalled("master_duel")` if no library holds it.
pub fn find_install() -> AppResult<PathBuf> {
    let steam_root = crate::steam::paths::get_steam_install_path()?;
    for lib in library_roots(&steam_root) {
        let candidate = lib
            .join("steamapps")
            .join("common")
            .join(INSTALL_DIR_NAME);
        if candidate.join("masterduel.exe").exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::GameNotInstalled("master_duel"))
}

/// `<install>/LocalData` — per-account folders live directly under here.
pub fn local_data(install: &Path) -> PathBuf {
    install.join("LocalData")
}

/// `<install>/LocalSave` — per-account save data (decks, replays, etc.).
pub fn local_save(install: &Path) -> PathBuf {
    install.join("LocalSave")
}

/// `<install>/LocalData/DATA/0000` — the shared asset cache that every
/// linked account's `0000` junction points at.
pub fn shared_cache(install: &Path) -> PathBuf {
    local_data(install).join("DATA").join("0000")
}

/// `<install>/accounts.csv` — the id↔label mapping.
pub fn accounts_csv(install: &Path) -> PathBuf {
    install.join("accounts.csv")
}
