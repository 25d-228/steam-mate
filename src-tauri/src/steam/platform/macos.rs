//! macOS Steam discovery, file-backed registry access, and process control.

use std::borrow::Cow;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use keyvalues_parser::{Obj, Value, Vdf};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, Signal, System};

use super::super::file::atomic_write;
use crate::error::{AppError, AppResult};
use crate::steam::account::SteamAccount;

const STEAM_PROCESS: &str = "steam_osx";
const STEAM_APP: &str = "Steam.app";
const STEAM_EXECUTABLE: &str = "Contents/MacOS/steam_osx";

fn data_dir_from_home(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("Steam")
}

pub fn steam_data_dir() -> AppResult<PathBuf> {
    let data_dir = dirs::home_dir()
        .map(|home| data_dir_from_home(&home))
        .ok_or(AppError::SteamNotInstalled)?;
    data_dir
        .is_dir()
        .then_some(data_dir)
        .ok_or(AppError::SteamNotInstalled)
}

fn find_install_dir(system_applications: &Path, user_applications: &Path) -> Option<PathBuf> {
    [system_applications, user_applications]
        .into_iter()
        .map(|applications| applications.join(STEAM_APP))
        .find(|candidate| candidate.join(STEAM_EXECUTABLE).is_file())
}

pub fn steam_install_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or(AppError::SteamNotInstalled)?;
    find_install_dir(Path::new("/Applications"), &home.join("Applications"))
        .ok_or(AppError::SteamNotInstalled)
}

pub fn steam_executable() -> AppResult<PathBuf> {
    Ok(steam_install_dir()?.join(STEAM_EXECUTABLE))
}

fn registry_path() -> AppResult<PathBuf> {
    Ok(steam_data_dir()?.join("registry.vdf"))
}

fn child_obj<'a>(object: &'a Obj<'_>, key: &str) -> Option<&'a Obj<'a>> {
    object.get(key)?.first()?.get_obj()
}

fn child_obj_mut<'a, 'text>(object: &'a mut Obj<'text>, key: &str) -> Option<&'a mut Obj<'text>> {
    object.get_mut(key)?.first_mut()?.get_mut_obj()
}

fn steam_registry<'a>(root: &'a Obj<'_>) -> Option<&'a Obj<'a>> {
    let hkcu = child_obj(root, "HKCU")?;
    let software = child_obj(hkcu, "Software")?;
    let valve = child_obj(software, "Valve")?;
    child_obj(valve, "Steam")
}

fn steam_registry_mut<'a, 'text>(root: &'a mut Obj<'text>) -> Option<&'a mut Obj<'text>> {
    let hkcu = child_obj_mut(root, "HKCU")?;
    let software = child_obj_mut(hkcu, "Software")?;
    let valve = child_obj_mut(software, "Valve")?;
    child_obj_mut(valve, "Steam")
}

fn parse_registry(text: &str) -> AppResult<Vdf<'_>> {
    let parsed =
        keyvalues_parser::parse(text).map_err(|error| AppError::VdfParse(error.to_string()))?;
    let registry = Vdf::from(parsed);
    if registry.key != "Registry" {
        return Err(AppError::VdfParse(
            "registry.vdf has no Registry root".into(),
        ));
    }
    Ok(registry)
}

fn read_auto_login_user(text: &str) -> AppResult<Option<String>> {
    let registry = parse_registry(text)?;
    let root = registry
        .value
        .get_obj()
        .ok_or_else(|| AppError::VdfParse("Registry is not an object".into()))?;
    let steam = steam_registry(root)
        .ok_or_else(|| AppError::VdfParse("Steam registry branch is missing".into()))?;
    Ok(steam
        .get("AutoLoginUser")
        .and_then(|values| values.first())
        .and_then(Value::get_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned))
}

fn set_registry_values(text: &str, auto_login_user: &str) -> AppResult<String> {
    let mut registry = parse_registry(text)?;
    let root = registry
        .value
        .get_mut_obj()
        .ok_or_else(|| AppError::VdfParse("Registry is not an object".into()))?;
    let steam = steam_registry_mut(root)
        .ok_or_else(|| AppError::VdfParse("Steam registry branch is missing".into()))?;

    steam.insert(
        Cow::Borrowed("AutoLoginUser"),
        vec![Value::Str(Cow::Owned(auto_login_user.to_owned()))],
    );
    if !auto_login_user.is_empty() {
        steam.insert(
            Cow::Borrowed("RememberPassword"),
            vec![Value::Str(Cow::Borrowed("1"))],
        );
    }
    Ok(registry.to_string())
}

fn get_auto_login_user_at(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    read_auto_login_user(&text).ok().flatten()
}

fn write_auto_login_user_at(path: &Path, account_name: &str) -> AppResult<()> {
    let text = fs::read_to_string(path).map_err(|error| {
        AppError::RegistryWrite(format!("cannot read {}: {error}", path.display()))
    })?;
    let updated = set_registry_values(&text, account_name)
        .map_err(|error| AppError::RegistryWrite(error.to_string()))?;
    atomic_write(path, &updated).map_err(|error| AppError::RegistryWrite(error.to_string()))
}

pub fn get_auto_login_user() -> Option<String> {
    get_auto_login_user_at(&registry_path().ok()?)
}

pub fn set_auto_login_user(account_name: &str) -> AppResult<()> {
    write_auto_login_user_at(&registry_path()?, account_name)
}

pub fn clear_auto_login_user() -> AppResult<()> {
    write_auto_login_user_at(&registry_path()?, "")
}

fn is_steam_process_name(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(STEAM_PROCESS))
}

pub fn is_steam_running() -> bool {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    sys.processes()
        .values()
        .any(|process| is_steam_process_name(process.name()))
}

fn resolve_current_account_from_state(
    accounts: &mut [SteamAccount],
    steam_running: bool,
    auto_login_user: Option<&str>,
) {
    let current_index =
        steam_running
            .then_some(auto_login_user)
            .flatten()
            .and_then(|account_name| {
                accounts
                    .iter()
                    .position(|account| account.account_name.eq_ignore_ascii_case(account_name))
            });

    for (index, account) in accounts.iter_mut().enumerate() {
        account.most_recent = current_index == Some(index);
    }
}

/// Reconcile Steam's remembered accounts with the live macOS session state.
///
/// Recent macOS clients do not reliably mark the logged-in user with
/// `MostRecent` in `loginusers.vdf`. While Steam is running, its file-backed
/// `AutoLoginUser` identifies the remembered account; while Steam is stopped,
/// no account is current.
pub fn resolve_current_account(accounts: &mut [SteamAccount]) {
    let steam_running = is_steam_running();
    let auto_login_user = steam_running.then(get_auto_login_user).flatten();
    resolve_current_account_from_state(accounts, steam_running, auto_login_user.as_deref());
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
        if is_steam_process_name(process.name()) {
            let _ = process.kill_with(Signal::Term);
        }
    }
    sleep(Duration::from_millis(800));

    if is_steam_running() {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        for process in sys.processes().values() {
            if is_steam_process_name(process.name()) {
                process.kill();
            }
        }
        sleep(Duration::from_millis(800));
    }

    if is_steam_running() {
        return Err(AppError::ProcessKillFailed(
            "steam_osx survived graceful shutdown and termination".into(),
        ));
    }
    Ok(())
}

pub fn start_steam(steam_exe: &Path) -> AppResult<()> {
    Command::new(steam_exe)
        .spawn()
        .map_err(|error| AppError::Io(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;

    use keyvalues_parser::{Obj, Value, Vdf};
    use tempfile::tempdir;

    use super::{
        data_dir_from_home, find_install_dir, get_auto_login_user_at, is_steam_process_name,
        read_auto_login_user, resolve_current_account_from_state, set_registry_values,
        steam_registry, write_auto_login_user_at,
    };
    use crate::steam::account::SteamAccount;

    const REGISTRY: &str = r#"
"Registry"
{
    "HKCU"
    {
        "Software"
        {
            "Valve"
            {
                "Steam"
                {
                    "AutoLoginUser" "old_login"
                    "Language" "japanese"
                    "RememberPassword" "0"
                }
                "OtherValveData" "keep"
            }
        }
    }
    "UnrelatedRoot" "keep-too"
}
"#;

    fn value_at<'a>(object: &'a Obj<'_>, key: &str) -> &'a str {
        object
            .get(key)
            .and_then(|values| values.first())
            .and_then(Value::get_str)
            .unwrap()
    }

    fn parsed_steam(text: &str) -> Vdf<'_> {
        Vdf::from(keyvalues_parser::parse(text).unwrap())
    }

    fn account(account_name: &str, most_recent: bool) -> SteamAccount {
        SteamAccount {
            account_name: account_name.to_owned(),
            persona_name: format!("{account_name} persona"),
            steam_id64: "76561198000000000".into(),
            steam_id32: 39734272,
            remember_password: true,
            most_recent,
            wants_offline_mode: false,
            skip_offline_mode_warning: false,
            allow_auto_login: true,
            timestamp: 0,
        }
    }

    #[test]
    fn resolves_data_dir_from_injected_home() {
        let home = Path::new("/tmp/example-home");
        assert_eq!(
            data_dir_from_home(home),
            home.join("Library/Application Support/Steam")
        );
    }

    #[test]
    fn resolves_install_dir_from_normal_application_locations() {
        let root = tempdir().unwrap();
        let system = root.path().join("Applications");
        let user = root.path().join("home/Applications");
        let user_app = user.join(super::STEAM_APP);
        let user_executable = user_app.join(super::STEAM_EXECUTABLE);
        fs::create_dir_all(user_executable.parent().unwrap()).unwrap();
        fs::write(&user_executable, "").unwrap();

        assert_eq!(find_install_dir(&system, &user), Some(user_app));

        let system_app = system.join(super::STEAM_APP);
        let system_executable = system_app.join(super::STEAM_EXECUTABLE);
        fs::create_dir_all(system_executable.parent().unwrap()).unwrap();
        fs::write(&system_executable, "").unwrap();
        assert_eq!(find_install_dir(&system, &user), Some(system_app));
    }

    #[test]
    fn matches_only_the_macos_steam_client_process_name() {
        assert!(is_steam_process_name(OsStr::new("steam_osx")));
        assert!(is_steam_process_name(OsStr::new("STEAM_OSX")));
        assert!(!is_steam_process_name(OsStr::new("steamwebhelper")));
        assert!(!is_steam_process_name(OsStr::new("steam_osx_helper")));
    }

    #[test]
    fn running_session_uses_auto_login_user_instead_of_raw_most_recent() {
        let mut accounts = vec![account("old_login", true), account("current_login", false)];

        resolve_current_account_from_state(&mut accounts, true, Some("CURRENT_LOGIN"));

        assert!(!accounts[0].most_recent);
        assert!(accounts[1].most_recent);
    }

    #[test]
    fn stopped_or_unresolved_session_has_no_current_account() {
        let mut accounts = vec![account("old_login", true), account("other_login", false)];

        resolve_current_account_from_state(&mut accounts, false, Some("old_login"));
        assert!(accounts.iter().all(|account| !account.most_recent));

        accounts[0].most_recent = true;
        resolve_current_account_from_state(&mut accounts, true, Some("missing_login"));
        assert!(accounts.iter().all(|account| !account.most_recent));
    }

    #[test]
    fn reads_sets_and_clears_auto_login_without_losing_other_values() {
        assert_eq!(
            read_auto_login_user(REGISTRY).unwrap().as_deref(),
            Some("old_login")
        );

        let set = set_registry_values(REGISTRY, "new_login").unwrap();
        assert_eq!(
            read_auto_login_user(&set).unwrap().as_deref(),
            Some("new_login")
        );
        let parsed = parsed_steam(&set);
        let root = parsed.value.get_obj().unwrap();
        let steam = steam_registry(root).unwrap();
        assert_eq!(value_at(steam, "RememberPassword"), "1");
        assert_eq!(value_at(steam, "Language"), "japanese");
        assert_eq!(value_at(root, "UnrelatedRoot"), "keep-too");

        let cleared = set_registry_values(&set, "").unwrap();
        assert_eq!(read_auto_login_user(&cleared).unwrap(), None);
        let parsed = parsed_steam(&cleared);
        let root = parsed.value.get_obj().unwrap();
        let steam = steam_registry(root).unwrap();
        assert_eq!(value_at(steam, "RememberPassword"), "1");
        assert_eq!(value_at(steam, "Language"), "japanese");
        assert_eq!(value_at(root, "UnrelatedRoot"), "keep-too");
    }

    #[test]
    fn updates_the_injected_registry_file_atomically() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("registry.vdf");
        fs::write(&path, REGISTRY).unwrap();

        write_auto_login_user_at(&path, "file_login").unwrap();
        assert_eq!(get_auto_login_user_at(&path).as_deref(), Some("file_login"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("japanese"));
        assert!(text.contains("keep-too"));
        assert!(!path.with_extension("vdf.steam-mate.tmp").exists());

        write_auto_login_user_at(&path, "").unwrap();
        assert_eq!(get_auto_login_user_at(&path), None);
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("japanese"));
        assert!(text.contains("keep-too"));
    }
}
