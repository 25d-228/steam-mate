//! Windows Steam discovery, registry access, and process control.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE};
use winreg::RegKey;

use crate::error::{AppError, AppResult};

const STEAM_PROCESS: &str = "steam.exe";
const STEAM_REG_PATH: &str = r"Software\Valve\Steam";

pub fn steam_data_dir() -> AppResult<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam")
        .map_err(|_| AppError::SteamNotInstalled)?;
    let path: String = key
        .get_value("InstallPath")
        .map_err(|_| AppError::SteamNotInstalled)?;
    Ok(PathBuf::from(path))
}

pub fn steam_executable() -> AppResult<PathBuf> {
    Ok(steam_install_dir()?.join("Steam.exe"))
}

pub fn steam_install_dir() -> AppResult<PathBuf> {
    steam_data_dir()
}

pub fn get_auto_login_user() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(STEAM_REG_PATH, KEY_QUERY_VALUE)
        .ok()?;
    let value: String = key.get_value("AutoLoginUser").ok()?;
    (!value.is_empty()).then_some(value)
}

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
        .map_err(|e| AppError::RegistryWrite(e.to_string()))
}

pub fn clear_auto_login_user() -> AppResult<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(STEAM_REG_PATH, KEY_SET_VALUE)
        .map_err(|e| AppError::RegistryWrite(e.to_string()))?;
    key.set_value("AutoLoginUser", &"".to_string())
        .map_err(|e| AppError::RegistryWrite(e.to_string()))
}

pub fn is_steam_running() -> bool {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    sys.processes().values().any(|process| {
        process
            .name()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(STEAM_PROCESS))
    })
}

pub fn stop_steam(steam_exe: &Path) -> AppResult<()> {
    if !is_steam_running() {
        return Ok(());
    }

    let _ = Command::new(steam_exe).arg("-shutdown").spawn();
    for _ in 0..100 {
        sleep(Duration::from_millis(150));
        if !is_steam_running() {
            sleep(Duration::from_millis(800));
            return Ok(());
        }
    }

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    for process in sys.processes().values() {
        if process
            .name()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(STEAM_PROCESS))
        {
            process.kill();
        }
    }
    sleep(Duration::from_millis(800));

    if is_steam_running() {
        return Err(AppError::ProcessKillFailed(
            "steam.exe survived graceful shutdown and kill".into(),
        ));
    }
    Ok(())
}

pub fn start_steam(steam_exe: &Path) -> AppResult<()> {
    Command::new(steam_exe)
        .spawn()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}
