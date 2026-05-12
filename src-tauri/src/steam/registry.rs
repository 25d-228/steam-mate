//! Steam-internal "registry" — Win HKCU on Windows, registry.vdf on Linux/Mac.
//! NOTE: this is NOT the Windows registry concept generically — it's the
//! specific Steam keys under HKCU\Software\Valve\Steam.

use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

use crate::error::{AppError, AppResult};

/// The Steam user-registry subkey path under HKCU.
const STEAM_REG_PATH: &str = r"Software\Valve\Steam";

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
