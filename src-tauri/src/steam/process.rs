//! Kill / launch the Steam process.

use std::path::Path;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::error::{AppError, AppResult};

/// The Steam executable's process name, matched case-insensitively.
const STEAM_PROCESS: &str = "steam.exe";

/// Whether a `steam.exe` process is currently running.
///
/// Takes a fresh snapshot of process *names only* — `ProcessRefreshKind::new()`
/// skips the per-process CPU/memory/disk collection a default refresh does,
/// which matters now that the frontend polls this every few seconds (same
/// pattern as the Master Duel probe). Matches case-insensitively.
pub fn is_steam_running() -> bool {
    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new()),
    );
    sys.processes().values().any(|p| {
        p.name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(STEAM_PROCESS))
    })
}

/// Stop Steam, gracefully if possible, forcefully if not. Idempotent.
///
/// No-op (returns `Ok`) if Steam isn't running. Otherwise asks Steam to shut
/// itself down via `steam.exe -shutdown` (ignoring any spawn error), then polls
/// for up to 3000 ms in 150 ms steps. If Steam is still alive after that, every
/// `steam.exe` process is `kill()`ed and we wait a final 500 ms; if it *still*
/// won't die we return [`AppError::ProcessKillFailed`].
pub fn kill_steam(steam_exe: &Path) -> AppResult<()> {
    if !is_steam_running() {
        return Ok(());
    }

    // Graceful: let Steam tear down its own children/overlay cleanly.
    let _ = Command::new(steam_exe).arg("-shutdown").spawn();

    // Poll up to 3000 ms in 150 ms steps for the graceful exit.
    for _ in 0..20 {
        sleep(Duration::from_millis(150));
        if !is_steam_running() {
            return Ok(());
        }
    }

    // Forceful: SIGKILL-equivalent on every remaining steam.exe.
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    for proc in sys.processes().values() {
        if proc
            .name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(STEAM_PROCESS))
        {
            proc.kill();
        }
    }
    sleep(Duration::from_millis(500));

    if is_steam_running() {
        return Err(AppError::ProcessKillFailed(
            "steam.exe survived graceful shutdown and kill".into(),
        ));
    }
    Ok(())
}

/// Launch Steam and return immediately (does not wait for it to come up).
///
/// Spawns `steam_exe` with no extra arguments; a spawn failure maps to
/// [`AppError::Io`].
pub fn start_steam(steam_exe: &Path) -> AppResult<()> {
    Command::new(steam_exe)
        .spawn()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}
