//! Locate Steam install dir / loginusers.vdf / etc. Windows-only: reads the
//! Steam install path from the Windows registry (`winreg`).

use std::path::PathBuf;

use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::error::{AppError, AppResult};

/// Read Steam's install directory from the Windows registry.
///
/// Steam (a 32-bit program) writes its install path to
/// `HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath`. Returns
/// `AppError::SteamNotInstalled` if the key or value is missing.
pub fn get_steam_install_path() -> AppResult<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam")
        .map_err(|_| AppError::SteamNotInstalled)?;
    let path: String = key
        .get_value("InstallPath")
        .map_err(|_| AppError::SteamNotInstalled)?;
    Ok(PathBuf::from(path))
}

/// Path to `<steam-install>\config\loginusers.vdf`.
///
/// Doesn't check whether the file actually exists — that's the
/// parser's concern (returns `AppError::SteamNotInstalled` if
/// the file is missing when read).
pub fn loginusers_vdf_path() -> AppResult<PathBuf> {
    Ok(get_steam_install_path()?
        .join("config")
        .join("loginusers.vdf"))
}

/// Path to `<steam-install>\config\config.vdf`.
///
/// This is the global Steam client config (distinct from the per-machine token
/// store in `%LocalAppData%\Steam\local.vdf`). steam-mate touches exactly one
/// key in it — `AlwaysShowUserChooser` — to keep silent auto-login working; see
/// [`super::switch::force_auto_login`]. Existence isn't checked here.
pub fn config_vdf_path() -> AppResult<PathBuf> {
    Ok(get_steam_install_path()?.join("config").join("config.vdf"))
}
