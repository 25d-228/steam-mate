//! Junction create/remove logic — the safety-critical heart of the module.
//!
//! Master Duel keeps a multi-gigabyte asset cache. To avoid duplicating it per
//! account we replace each account's `0000` asset folder with an NTFS
//! *junction* pointing at one shared cache. Junctions (unlike symlinks) don't
//! require admin to create on NTFS.
//!
//! The one rule that must never be broken: `fs::remove_dir` on a junction
//! deletes only the link, while `fs::remove_dir_all` would *follow* the
//! junction and recursively delete the shared cache behind it. Every unlink/
//! delete path here is written to remove junctions with `fs::remove_dir` first.
//!
//! All functions take explicit paths (no global install lookup) so tests can
//! drive them entirely inside a `tempfile::tempdir()`.

use std::path::Path;

use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};

/// Master Duel's process image name.
const PROCESS_NAME: &str = "masterduel.exe";

/// Windows `ERROR_NOT_A_REPARSE_POINT` — returned when querying reparse data
/// on a path that exists but isn't a junction/symlink (i.e. a real directory).
const ERROR_NOT_A_REPARSE_POINT: i32 = 4390;

/// Whether `acct_0000` is currently a junction. Missing path ⇒ `Ok(false)`.
///
/// `junction::exists` errors with `ERROR_NOT_A_REPARSE_POINT` for a real
/// (non-junction) directory; we treat that as "not linked" rather than an
/// error, so only genuine IO failures propagate.
pub fn is_linked(acct_0000: &Path) -> AppResult<bool> {
    if !acct_0000.exists() {
        return Ok(false);
    }
    match junction::exists(acct_0000) {
        Ok(linked) => Ok(linked),
        Err(e) if e.raw_os_error() == Some(ERROR_NOT_A_REPARSE_POINT) => Ok(false),
        Err(e) => Err(AppError::from(e)),
    }
}

/// Whether `masterduel.exe` is currently running (case-insensitive match).
pub fn is_running() -> bool {
    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new()),
    );
    sys.processes().values().any(|p| {
        p.name()
            .to_string_lossy()
            .eq_ignore_ascii_case(PROCESS_NAME)
    })
}

/// Point an account's `0000` folder at the shared cache via a junction.
///
/// Preconditions: `shared` must exist (else `GameNotInstalled`); the parent of
/// `acct_0000` must exist (else `AccountNotFound`). If `acct_0000` is already a
/// junction this is a no-op. If it's a real directory we refuse while the game
/// is `running` (`GameRunning`); if it holds any file and `force` is false we
/// refuse (`JunctionFailed`); otherwise we delete it and create the junction.
pub fn link_account(
    shared: &Path,
    acct_0000: &Path,
    force: bool,
    running: bool,
) -> AppResult<()> {
    if !shared.exists() {
        return Err(AppError::GameNotInstalled("master_duel"));
    }
    let parent = acct_0000
        .parent()
        .ok_or_else(|| AppError::AccountNotFound(acct_0000.display().to_string()))?;
    if !parent.exists() {
        return Err(AppError::AccountNotFound(parent.display().to_string()));
    }

    // Already linked — nothing to do.
    if junction::exists(acct_0000).unwrap_or(false) {
        return Ok(());
    }

    if acct_0000.exists() {
        if running {
            return Err(AppError::GameRunning("master_duel"));
        }
        let has_files = WalkDir::new(acct_0000)
            .into_iter()
            .filter_map(Result::ok)
            .any(|e| e.file_type().is_file());
        if has_files && !force {
            return Err(AppError::JunctionFailed(
                "0000 contains files; pass force".into(),
            ));
        }
        std::fs::remove_dir_all(acct_0000)?;
    }

    junction::create(shared, acct_0000).map_err(AppError::from)
}

/// Build the empty `0000` folder skeleton the game expects at `dir`.
///
/// Creates `dir` itself plus the 256 two-hex-digit subdirectories `00`..=`ff`
/// and a `root` folder — exactly what a freshly-unlinked or freshly-created
/// shared cache holds before the game writes any assets. Shared by
/// [`unlink_account`] (rebuilding an account's own folder) and
/// [`create_cache`] (laying down an empty shared cache).
pub fn build_skeleton(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    for n in 0u16..=0xff {
        std::fs::create_dir_all(dir.join(format!("{n:02x}")))?;
    }
    std::fs::create_dir_all(dir.join("root"))?;
    Ok(())
}

/// Replace a `0000` junction with a fresh, empty real folder structure.
///
/// `acct_0000` must currently be a junction (`JunctionFailed("not a
/// junction")` otherwise). Refuses while the game is `running`. Removes the
/// junction with `fs::remove_dir` — NEVER `remove_dir_all`, which would follow
/// the link into the shared cache — then recreates the directory plus the 256
/// hex subdirectories `00`..=`ff` and a `root` folder that the game expects
/// (via the shared [`build_skeleton`]).
pub fn unlink_account(acct_0000: &Path, running: bool) -> AppResult<()> {
    if !junction::exists(acct_0000).unwrap_or(false) {
        return Err(AppError::JunctionFailed("not a junction".into()));
    }
    if running {
        return Err(AppError::GameRunning("master_duel"));
    }

    // Remove the link only — do not recurse into the shared target.
    std::fs::remove_dir(acct_0000)?;

    build_skeleton(acct_0000)
}

/// Create the shared cache at `shared` (`<install>/LocalData/DATA/0000`).
///
/// Refuses while the game is `running` ([`AppError::GameRunning`]); refuses if
/// `shared` already exists ([`AppError::JunctionFailed`]) so an existing cache
/// is never clobbered. Two modes:
///
/// - `seed = Some(seed_0000)`: the seed account's real `0000` folder is *moved*
///   (renamed) into place as the new shared cache, then a junction is created
///   back at the seed's old `0000` location pointing at it. A rename on one
///   volume is instant and copies nothing. If the junction can't be created the
///   move is reverted (the folder renamed back) before the error returns, so the
///   seed is never stranded. `seed_0000` must be a real directory (its parent is
///   the seed account folder).
/// - `seed = None`: an empty skeleton is laid down at `shared` via
///   [`build_skeleton`] (root + the 256 hex subfolders).
///
/// `shared`'s parent (`.../DATA`) is created as needed in both modes. Like the
/// other mutators here, `running` is passed in (the command layer supplies
/// [`is_running`]) so tests can drive it without a live process.
pub fn create_cache(shared: &Path, seed: Option<&Path>, running: bool) -> AppResult<()> {
    if running {
        return Err(AppError::GameRunning("master_duel"));
    }
    if shared.exists() {
        return Err(AppError::JunctionFailed(
            "shared cache already exists".into(),
        ));
    }
    // Ensure the DATA parent exists for the rename / skeleton target.
    let data_dir = shared
        .parent()
        .ok_or_else(|| AppError::JunctionFailed("shared cache has no parent".into()))?;
    std::fs::create_dir_all(data_dir)?;

    match seed {
        Some(seed_0000) => {
            // Move the seed's own assets into the shared location (instant,
            // same-volume rename — nothing copied)...
            std::fs::rename(seed_0000, shared)?;
            // ...then point the seed's old 0000 at the new shared cache. If the
            // junction can't be made, undo the move so the seed isn't stranded.
            if let Err(e) = junction::create(shared, seed_0000) {
                // junction::create makes the directory first and sets the
                // reparse point second, so a failure can leave a plain empty
                // dir at seed_0000 — which would block the rename-back. Clear
                // it first: remove_dir is safe on an empty dir or a half-made
                // junction and never recurses into the moved cache.
                let _ = std::fs::remove_dir(seed_0000);
                if std::fs::rename(shared, seed_0000).is_err() {
                    return Err(AppError::JunctionFailed(format!(
                        "junction failed and the cache could not be moved back \
                         — it is intact at {} (move it back by hand): {e}",
                        shared.display()
                    )));
                }
                return Err(AppError::from(e));
            }
            Ok(())
        }
        None => build_skeleton(shared),
    }
}

/// Permanently delete an account folder (LocalData) and its saves (LocalSave).
///
/// Refuses while the game is `running`. `local_data/folder_id` must exist
/// (`AccountNotFound` otherwise). To stay safe we FIRST walk the account's
/// direct children and `fs::remove_dir` any that are junctions (so the
/// recursive delete can never follow a link into the shared cache), THEN
/// `fs::remove_dir_all` the account folder, THEN delete its save folder under
/// `local_save` if present. CSV row removal is the caller's responsibility.
pub fn delete_account(
    local_data: &Path,
    local_save: &Path,
    folder_id: &str,
    running: bool,
) -> AppResult<()> {
    if running {
        return Err(AppError::GameRunning("master_duel"));
    }
    let acct = local_data.join(folder_id);
    if !acct.exists() {
        return Err(AppError::AccountNotFound(folder_id.to_string()));
    }

    // Unlink any junctioned children before the recursive delete.
    for entry in std::fs::read_dir(&acct)? {
        let child = entry?.path();
        if junction::exists(&child).unwrap_or(false) {
            std::fs::remove_dir(&child)?;
        }
    }
    std::fs::remove_dir_all(&acct)?;

    let save = local_save.join(folder_id);
    if save.exists() {
        std::fs::remove_dir_all(&save)?;
    }
    Ok(())
}

/// Scan `local_data` for profiles that could seed a new shared cache.
///
/// Returns `(folder_id, size_bytes)` for each profile whose own `0000` is a
/// REAL directory (not a junction — a linked profile already points at a cache)
/// holding at least one file (an empty `0000` carries no cache to move). The
/// shared `DATA` folder is skipped, as are non-directory entries. Sorted by
/// `size_bytes` descending so the most complete cache is offered first. The
/// caller joins each `folder_id` to its `accounts.csv` label; the junction +
/// size logic lives here so it can be exercised in a tempdir.
pub fn seed_candidates(local_data: &Path) -> AppResult<Vec<(String, u64)>> {
    let mut candidates = Vec::new();
    if local_data.exists() {
        for entry in std::fs::read_dir(local_data)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let folder_id = entry.file_name().to_string_lossy().into_owned();
            // The shared cache lives under DATA, not a seedable profile.
            if folder_id == "DATA" {
                continue;
            }
            let acct_0000 = entry.path().join("0000");
            // Skip linked (already points at a cache) and absent folders.
            if is_linked(&acct_0000).unwrap_or(false) || !acct_0000.exists() {
                continue;
            }
            // Need at least one real file to seed a cache from.
            let size_bytes = cache_size(&acct_0000);
            if size_bytes == 0 {
                continue;
            }
            candidates.push((folder_id, size_bytes));
        }
    }
    // Largest cache first — seed from the most complete profile.
    candidates.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    Ok(candidates)
}

/// Whether `acct_0000` holds at least one real file — i.e. it is an actual
/// cache copy, not the empty skeleton an unlink leaves behind (the skeleton is
/// 257 directories with no files). Short-circuits at the first file found, so
/// a full 13 GB cache answers immediately. Callers must check `is_linked`
/// first: walking a junction would happily count the shared cache's files.
pub fn has_files(acct_0000: &Path) -> bool {
    WalkDir::new(acct_0000)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_file())
}

/// Total byte size of the shared cache (sum of file lengths). 0 if missing.
pub fn cache_size(shared: &Path) -> u64 {
    if !shared.exists() {
        return 0;
    }
    WalkDir::new(shared)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a shared-cache dir with a sentinel file, return its path.
    fn make_shared(root: &Path) -> std::path::PathBuf {
        let shared = root.join("shared");
        fs::create_dir_all(&shared).unwrap();
        fs::write(shared.join("sentinel.bin"), b"precious cache data").unwrap();
        shared
    }

    #[test]
    fn link_then_is_linked_true() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        let acct_parent = dir.path().join("acct");
        fs::create_dir_all(&acct_parent).unwrap();
        let acct_0000 = acct_parent.join("0000");

        assert!(!is_linked(&acct_0000).unwrap());
        link_account(&shared, &acct_0000, false, false).unwrap();
        assert!(is_linked(&acct_0000).unwrap());
        // The junction resolves to the shared sentinel.
        assert!(acct_0000.join("sentinel.bin").exists());
    }

    #[test]
    fn link_missing_shared_errors() {
        let dir = tempfile::tempdir().unwrap();
        let acct_parent = dir.path().join("acct");
        fs::create_dir_all(&acct_parent).unwrap();
        let err = link_account(
            &dir.path().join("nope"),
            &acct_parent.join("0000"),
            false,
            false,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::GameNotInstalled("master_duel")));
    }

    #[test]
    fn link_missing_parent_is_account_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        // parent dir intentionally absent
        let acct_0000 = dir.path().join("absent").join("0000");
        let err = link_account(&shared, &acct_0000, false, false).unwrap_err();
        assert!(matches!(err, AppError::AccountNotFound(_)));
    }

    #[test]
    fn link_refuses_nonempty_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        let acct_0000 = dir.path().join("0000");
        fs::create_dir_all(&acct_0000).unwrap();
        fs::write(acct_0000.join("local.dat"), b"unsynced").unwrap();

        let err = link_account(&shared, &acct_0000, false, false).unwrap_err();
        assert!(matches!(err, AppError::JunctionFailed(_)));
        // With force it succeeds, replacing the folder with a junction.
        link_account(&shared, &acct_0000, true, false).unwrap();
        assert!(is_linked(&acct_0000).unwrap());
    }

    #[test]
    fn link_running_guard() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        let acct_0000 = dir.path().join("0000");
        fs::create_dir_all(&acct_0000).unwrap();
        let err = link_account(&shared, &acct_0000, true, true).unwrap_err();
        assert!(matches!(err, AppError::GameRunning("master_duel")));
    }

    #[test]
    fn unlink_keeps_shared_intact_and_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        let acct_0000 = dir.path().join("0000");

        link_account(&shared, &acct_0000, false, false).unwrap();
        assert!(is_linked(&acct_0000).unwrap());

        unlink_account(&acct_0000, false).unwrap();

        // Shared sentinel must survive (remove_dir, not remove_dir_all).
        assert!(shared.join("sentinel.bin").exists());
        // No longer a junction; rebuilt as a real dir.
        assert!(!is_linked(&acct_0000).unwrap());
        // 256 hex dirs + "root" = 257 entries.
        let count = fs::read_dir(&acct_0000).unwrap().count();
        assert_eq!(count, 257);
    }

    #[test]
    fn unlink_non_junction_errors() {
        let dir = tempfile::tempdir().unwrap();
        let acct_0000 = dir.path().join("0000");
        fs::create_dir_all(&acct_0000).unwrap();
        let err = unlink_account(&acct_0000, false).unwrap_err();
        assert!(matches!(err, AppError::JunctionFailed(msg) if msg == "not a junction"));
    }

    #[test]
    fn delete_with_linked_child_keeps_shared() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        let local_data = dir.path().join("LocalData");
        let local_save = dir.path().join("LocalSave");
        let folder_id = "0000000000000001";

        let acct = local_data.join(folder_id);
        fs::create_dir_all(&acct).unwrap();
        // Link the account's 0000 child to the shared cache.
        let child = acct.join("0000");
        link_account(&shared, &child, false, false).unwrap();
        // Give it a save folder too.
        fs::create_dir_all(local_save.join(folder_id)).unwrap();
        fs::write(local_save.join(folder_id).join("deck.bin"), b"x").unwrap();

        delete_account(&local_data, &local_save, folder_id, false).unwrap();

        // Account + save are gone...
        assert!(!acct.exists());
        assert!(!local_save.join(folder_id).exists());
        // ...but the shared cache the junction pointed at is untouched.
        assert!(shared.join("sentinel.bin").exists());
    }

    #[test]
    fn delete_missing_account_errors() {
        let dir = tempfile::tempdir().unwrap();
        let local_data = dir.path().join("LocalData");
        let local_save = dir.path().join("LocalSave");
        fs::create_dir_all(&local_data).unwrap();
        let err =
            delete_account(&local_data, &local_save, "nope", false).unwrap_err();
        assert!(matches!(err, AppError::AccountNotFound(_)));
    }

    #[test]
    fn delete_running_guard() {
        let dir = tempfile::tempdir().unwrap();
        let local_data = dir.path().join("LocalData");
        let local_save = dir.path().join("LocalSave");
        fs::create_dir_all(local_data.join("id")).unwrap();
        let err =
            delete_account(&local_data, &local_save, "id", true).unwrap_err();
        assert!(matches!(err, AppError::GameRunning("master_duel")));
    }

    #[test]
    fn cache_size_sums_files() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared");
        fs::create_dir_all(shared.join("sub")).unwrap();
        fs::write(shared.join("a.bin"), b"12345").unwrap();
        fs::write(shared.join("sub").join("b.bin"), b"678").unwrap();
        assert_eq!(cache_size(&shared), 8);
        // Missing dir ⇒ 0.
        assert_eq!(cache_size(&dir.path().join("absent")), 0);
    }

    #[test]
    fn build_skeleton_creates_root_and_256_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("0000");
        build_skeleton(&target).unwrap();
        // 256 hex dirs + "root" = 257 entries.
        assert_eq!(fs::read_dir(&target).unwrap().count(), 257);
        assert!(target.join("00").is_dir());
        assert!(target.join("ff").is_dir());
        assert!(target.join("root").is_dir());
    }

    #[test]
    fn create_cache_empty_lays_down_skeleton() {
        let dir = tempfile::tempdir().unwrap();
        // DATA does not exist yet — create_cache must make it then the skeleton.
        let shared = dir.path().join("LocalData").join("DATA").join("0000");
        create_cache(&shared, None, false).unwrap();
        assert!(shared.is_dir());
        assert_eq!(fs::read_dir(&shared).unwrap().count(), 257);
    }

    #[test]
    fn create_cache_refuses_when_shared_exists() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("DATA").join("0000");
        fs::create_dir_all(&shared).unwrap();
        let err = create_cache(&shared, None, false).unwrap_err();
        assert!(matches!(err, AppError::JunctionFailed(_)));
    }

    #[test]
    fn create_cache_refuses_when_running() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("DATA").join("0000");
        let err = create_cache(&shared, None, true).unwrap_err();
        assert!(matches!(err, AppError::GameRunning("master_duel")));
        // Nothing was created.
        assert!(!shared.exists());
    }

    #[test]
    fn seed_candidates_returns_unlinked_with_files_and_skips_others() {
        let dir = tempfile::tempdir().unwrap();
        let shared = make_shared(dir.path());
        let local_data = dir.path().join("LocalData");

        // A: unlinked 0000 holding a real file — a candidate.
        let a0000 = local_data.join("aaaa").join("0000");
        fs::create_dir_all(&a0000).unwrap();
        fs::write(a0000.join("big.bin"), b"0123456789").unwrap(); // 10 bytes

        // B: unlinked 0000 holding more bytes — a larger candidate.
        let b0000 = local_data.join("bbbb").join("0000");
        fs::create_dir_all(&b0000).unwrap();
        fs::write(b0000.join("big.bin"), b"0123456789abcdef").unwrap(); // 16 bytes

        // C: linked 0000 (junction into shared) — skipped.
        let c0000 = local_data.join("cccc").join("0000");
        fs::create_dir_all(c0000.parent().unwrap()).unwrap();
        link_account(&shared, &c0000, false, false).unwrap();
        assert!(is_linked(&c0000).unwrap());

        // D: empty 0000 (no files) — skipped.
        let d0000 = local_data.join("dddd").join("0000");
        fs::create_dir_all(&d0000).unwrap();

        // DATA: the shared cache folder itself — never a candidate.
        fs::create_dir_all(local_data.join("DATA").join("0000")).unwrap();
        fs::write(local_data.join("DATA").join("0000").join("x.bin"), b"z").unwrap();

        let cands = seed_candidates(&local_data).unwrap();
        let ids: Vec<&str> = cands.iter().map(|(id, _)| id.as_str()).collect();
        // Only the two unlinked-with-files profiles, largest first.
        assert_eq!(ids, vec!["bbbb", "aaaa"]);
        assert_eq!(cands[0].1, 16);
        assert_eq!(cands[1].1, 10);
    }

    #[test]
    fn create_cache_seed_moves_folder_and_links_back() {
        let dir = tempfile::tempdir().unwrap();
        // A seed profile whose own 0000 holds a sentinel file.
        let seed_0000 = dir.path().join("LocalData").join("seed").join("0000");
        fs::create_dir_all(&seed_0000).unwrap();
        fs::write(seed_0000.join("sentinel.bin"), b"precious cache data").unwrap();

        let shared = dir.path().join("LocalData").join("DATA").join("0000");
        create_cache(&shared, Some(&seed_0000), false).unwrap();

        // The sentinel now lives in the shared cache (folder was moved, not copied).
        assert!(shared.join("sentinel.bin").exists());
        // The seed's old 0000 is now a junction...
        assert!(is_linked(&seed_0000).unwrap());
        // ...and reading through it sees the moved sentinel.
        assert!(seed_0000.join("sentinel.bin").exists());
    }
}
