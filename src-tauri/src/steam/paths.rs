//! Locate Steam install dir / loginusers.vdf / etc., per OS.

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
