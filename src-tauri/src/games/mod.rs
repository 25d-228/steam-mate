//! Games module — per-game features. Each game is conditionally compiled
//! per its supported OS. Master Duel is Windows-only.

use crate::error::AppResult;

#[cfg(windows)]
pub mod master_duel;

/// One supported game, as surfaced to the frontend.
///
/// `installed` reflects whether the game was detected on disk at the time of
/// the call. The set of entries is platform-dependent (e.g. Master Duel is
/// Windows-only), so the frontend should render whatever this returns rather
/// than assuming a fixed list.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub installed: bool,
}

/// List the games this build supports, with per-game install detection.
///
/// On Windows this probes for Master Duel via
/// [`master_duel::paths::find_install`] (`installed = is_ok()`). On other
/// platforms there are currently no supported games, so it returns an empty
/// vec. Defined for all platforms so the command can always be registered.
#[tauri::command]
pub async fn list_supported_games() -> AppResult<Vec<GameInfo>> {
    #[cfg(windows)]
    {
        let installed = master_duel::paths::find_install().is_ok();
        Ok(vec![GameInfo {
            id: "master_duel",
            display_name: "Yu-Gi-Oh! Master Duel",
            installed,
        }])
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}
