//! Steam-internal "registry" — the Steam keys under HKCU\Software\Valve\Steam.
//! NOTE: this is NOT the Windows registry concept generically — it's that
//! specific subkey. Windows-only (uses `winreg`).

use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

use crate::error::{AppError, AppResult};

/// The Steam user-registry subkey path under HKCU.
const STEAM_REG_PATH: &str = r"Software\Valve\Steam";

/// Remember `account_name` as the auto-login user so Steam signs straight in
/// on its next launch.
///
/// Writes three values under `HKCU\Software\Valve\Steam`:
/// `AutoLoginUser` (the global key) and `AutoLoginUser_steamchina` (the
/// Steam China client's separate key) both to `account_name` as `REG_SZ`, plus
/// `RememberPassword = 1` as `REG_DWORD`. Any failure maps to
/// [`AppError::RegistryWrite`].
pub fn set_auto_login_user(account_name: &str) -> AppResult<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(STEAM_REG_PATH, KEY_SET_VALUE)
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    key.set_value("AutoLoginUser", &account_name.to_string())
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    key.set_value("AutoLoginUser_steamchina", &account_name.to_string())
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    key.set_value("RememberPassword", &1u32)
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    Ok(())
}

/// Clear the "remembered auto-login user" so Steam launches at the login screen
/// next time.
///
/// Writes an empty string to `HKCU\Software\Valve\Steam\AutoLoginUser`. The
/// key is left in place (Steam recreates it on next run anyway); only the
/// value is blanked.
pub fn clear_auto_login_user() -> AppResult<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(STEAM_REG_PATH, KEY_SET_VALUE)
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    key.set_value("AutoLoginUser", &"".to_string())
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    Ok(())
}
