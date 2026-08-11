//! Native macOS menu-bar and Dock quick switching.
//!
//! Tauri owns the status item. AppKit owns the Dock menu: its public
//! `applicationDockMenu:` delegate hook is added narrowly to Tauri's existing
//! application-delegate class, without replacing that delegate or replacing
//! any method. Both surfaces render the same pure [`MenuModel`] and dispatch
//! the same stable action IDs.

use std::collections::HashMap;
use std::error::Error;
use std::ffi::c_char;
use std::ptr;
use std::sync::{Mutex, MutexGuard, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, NSObjectProtocol, Sel};
use objc2::{define_class, ffi, msg_send, sel, DeclaredClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem,
};
use objc2_foundation::{MainThreadMarker, NSString};
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::steam::account::{self, SteamAccount};
use crate::steam::{platform, switch};

const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "macos-native-quick-switch";
const OPEN_ID: &str = "native-open-steam-mate";
const ACCOUNT_ID_PREFIX: &str = "native-steam-account-";

static CONTROLLER: OnceLock<NativeController> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
enum NativeAction {
    Open,
    Switch(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActionEntry {
    id: String,
    label: String,
    enabled: bool,
    checked: bool,
    action: NativeAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MenuEntry {
    Action(ActionEntry),
    Separator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuModel {
    entries: Vec<MenuEntry>,
}

impl MenuModel {
    fn empty() -> Self {
        Self {
            entries: vec![MenuEntry::Action(ActionEntry {
                id: OPEN_ID.into(),
                label: "Open steam-mate".into(),
                enabled: true,
                checked: false,
                action: NativeAction::Open,
            })],
        }
    }

    fn from_accounts(accounts: &[SteamAccount], steam_running: bool, busy: bool) -> Self {
        let mut model = Self::empty();
        if !accounts.is_empty() {
            model.entries.push(MenuEntry::Separator);
        }

        for (index, account) in accounts.iter().enumerate() {
            let current = steam_running && account.most_recent;
            model.entries.push(MenuEntry::Action(ActionEntry {
                id: format!("{ACCOUNT_ID_PREFIX}{index}"),
                label: format!("{} ({})", account.persona_name, account.account_name),
                enabled: !busy && !current,
                checked: current,
                action: NativeAction::Switch(account.account_name.clone()),
            }));
        }
        model
    }

    fn with_busy(&self, busy: bool) -> Self {
        let mut model = self.clone();
        for entry in &mut model.entries {
            let MenuEntry::Action(entry) = entry else {
                continue;
            };
            if matches!(entry.action, NativeAction::Switch(_)) {
                entry.enabled = !busy && !entry.checked;
            }
        }
        model
    }

    fn actions(&self) -> HashMap<String, NativeAction> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Action(entry) if entry.enabled => {
                    Some((entry.id.clone(), entry.action.clone()))
                }
                _ => None,
            })
            .collect()
    }
}

struct ControllerState {
    busy: bool,
    model: MenuModel,
}

impl ControllerState {
    fn begin(&mut self, action_id: &str) -> Option<NativeAction> {
        let action = self.model.actions().get(action_id)?.clone();
        if matches!(action, NativeAction::Switch(_)) {
            if self.busy {
                return None;
            }
            self.busy = true;
            self.model = self.model.with_busy(true);
        }
        Some(action)
    }
}

struct NativeController {
    app: AppHandle,
    tray: TrayIcon<Wry>,
    state: Mutex<ControllerState>,
    refresh_lock: Mutex<()>,
}

impl NativeController {
    fn state(&self) -> MutexGuard<'_, ControllerState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn refresh_guard(&self) -> MutexGuard<'_, ()> {
        self.refresh_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn load_model(&self, busy: bool) -> Result<MenuModel, String> {
        let accounts = account::list_accounts().map_err(|error| error.to_string())?;
        let steam_running = platform::is_steam_running();
        Ok(MenuModel::from_accounts(&accounts, steam_running, busy))
    }

    /// Refresh the shared snapshot, optionally replacing the status-item menu.
    ///
    /// Dock callbacks are already on AppKit's main thread, so they refresh the
    /// model only and build the returned Dock menu directly. Other refreshes
    /// also replace the Tauri tray menu. A transient read failure keeps the
    /// last snapshot, changing only its busy availability.
    fn refresh(&self, update_tray: bool) {
        let _refresh = self.refresh_guard();
        let busy = self.state().busy;
        let model = self
            .load_model(busy)
            .unwrap_or_else(|_| self.state().model.with_busy(busy));
        self.state().model = model.clone();

        if update_tray {
            if let Ok(menu) = build_tauri_menu(&self.app, &model) {
                let _ = self.tray.set_menu(Some(menu));
            }
        }
    }

    fn cached_model(&self) -> MenuModel {
        self.state().model.clone()
    }

    fn dispatch(&self, action_id: &str) {
        let action = self.state().begin(action_id);
        match action {
            Some(NativeAction::Open) => focus_main_window(&self.app),
            Some(NativeAction::Switch(account_name)) => {
                // Reflect the in-flight state on both surfaces before starting
                // blocking Steam orchestration. Dock menus are rebuilt from
                // this same cached model when next requested.
                let model = self.cached_model();
                if let Ok(menu) = build_tauri_menu(&self.app, &model) {
                    let _ = self.tray.set_menu(Some(menu));
                }

                let app = self.app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        switch::switch_account(&account_name, false)
                    })
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result.map_err(|error| error.to_string()));

                    let Some(controller) = CONTROLLER.get() else {
                        return;
                    };
                    controller.state().busy = false;
                    controller.refresh(true);
                    let _ = app.emit("accounts-changed", ());
                    if let Err(error) = result {
                        let _ = app.emit("switch-error", error);
                    }
                });
            }
            None => {}
        }
    }
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tauri_menu(app: &AppHandle, model: &MenuModel) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for entry in &model.entries {
        match entry {
            MenuEntry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            MenuEntry::Action(entry) => {
                if matches!(entry.action, NativeAction::Switch(_)) {
                    let item = CheckMenuItemBuilder::with_id(&entry.id, &entry.label)
                        .enabled(entry.enabled)
                        .checked(entry.checked)
                        .build(app)?;
                    menu.append(&item)?;
                } else {
                    let item = MenuItemBuilder::with_id(&entry.id, &entry.label)
                        .enabled(entry.enabled)
                        .build(app)?;
                    menu.append(&item)?;
                }
            }
        }
    }
    Ok(menu)
}

define_class!(
    /// A Dock menu item that retains its stable action ID and dispatches itself.
    ///
    /// AppKit's `target` property is weak, while the menu retains its items.
    /// Making the item its own target therefore keeps the action target alive
    /// for exactly as long as the returned Dock menu.
    #[unsafe(super(NSMenuItem))]
    #[name = "SteamMateDockMenuItem"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Retained<NSString>]
    struct DockMenuItem;

    impl DockMenuItem {
        #[unsafe(method(performSteamMateMenuAction:))]
        fn perform_action(&self, _sender: Option<&AnyObject>) {
            if let Some(controller) = CONTROLLER.get() {
                controller.dispatch(&self.ivars().to_string());
            }
        }
    }

    unsafe impl NSObjectProtocol for DockMenuItem {}
);

impl DockMenuItem {
    fn new(mtm: MainThreadMarker, entry: &ActionEntry) -> Retained<Self> {
        let title = NSString::from_str(&entry.label);
        let key_equivalent = NSString::from_str("");
        let item = mtm.alloc().set_ivars(NSString::from_str(&entry.id));
        let item: Retained<Self> = unsafe {
            msg_send![super(item), initWithTitle: &*title, action: Some(sel!(performSteamMateMenuAction:)), keyEquivalent: &*key_equivalent]
        };
        item.setEnabled(entry.enabled);
        item.setState(if entry.checked {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        unsafe {
            item.setTarget(Some(&*item));
        }
        item
    }
}

fn build_dock_menu(model: &MenuModel, mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in &model.entries {
        match entry {
            MenuEntry::Separator => {
                let separator = NSMenuItem::separatorItem(mtm);
                menu.addItem(&separator);
            }
            MenuEntry::Action(entry) => {
                let item = DockMenuItem::new(mtm, entry);
                menu.addItem(&item);
            }
        }
    }
    menu
}

/// Public `NSApplicationDelegate.applicationDockMenu:` implementation.
///
/// AppKit invokes this immediately before presenting the Dock menu, giving us
/// the required presentation-time refresh point.
unsafe extern "C-unwind" fn application_dock_menu(
    _delegate: &AnyObject,
    _selector: Sel,
    _application: &NSApplication,
) -> *mut NSMenu {
    let Some(mtm) = MainThreadMarker::new() else {
        return ptr::null_mut();
    };
    let Some(controller) = CONTROLLER.get() else {
        return ptr::null_mut();
    };
    controller.refresh(false);
    Retained::autorelease_ptr(build_dock_menu(&controller.cached_model(), mtm))
}

/// Add only the missing public Dock-menu selector to Tauri's delegate class.
///
/// This is intentionally not a delegate replacement and not method swizzling:
/// setup refuses to replace an existing implementation.
fn install_dock_menu_hook() -> Result<(), Box<dyn Error>> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| std::io::Error::other("Dock menu setup must run on the main thread"))?;
    let application = NSApplication::sharedApplication(mtm);
    let delegate = application
        .delegate()
        .ok_or_else(|| std::io::Error::other("NSApplication has no delegate"))?;
    let selector = sel!(applicationDockMenu:);
    let delegate_object: &AnyObject = AsRef::<AnyObject>::as_ref(&*delegate);
    let class = delegate_object.class();
    if class.instance_method(selector).is_some() {
        return Err(std::io::Error::other(
            "the application delegate already implements applicationDockMenu:",
        )
        .into());
    }

    let implementation: Imp = unsafe {
        std::mem::transmute::<
            unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication) -> *mut NSMenu,
            Imp,
        >(application_dock_menu)
    };
    let type_encoding = b"@@:@\0";
    let added = unsafe {
        ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            selector,
            implementation,
            type_encoding.as_ptr().cast::<c_char>(),
        )
    };
    if !added.as_bool() {
        return Err(std::io::Error::other("failed to install applicationDockMenu:").into());
    }
    Ok(())
}

/// Build exactly one status item and install the Dock menu provider.
pub fn setup(app: &AppHandle) -> Result<(), Box<dyn Error>> {
    let initial_model = account::list_accounts()
        .map(|accounts| MenuModel::from_accounts(&accounts, platform::is_steam_running(), false))
        .unwrap_or_else(|_| MenuModel::empty());
    let menu = build_tauri_menu(app, &initial_model)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("steam-mate")
        .menu(&menu)
        .on_menu_event(|_, event| {
            if let Some(controller) = CONTROLLER.get() {
                controller.dispatch(event.id().as_ref());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        // The current icon has an opaque dark tile, so treating it as a
        // monochrome template would collapse it into an unreadable block.
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;

    CONTROLLER
        .set(NativeController {
            app: app.clone(),
            tray,
            state: Mutex::new(ControllerState {
                busy: false,
                model: initial_model,
            }),
            refresh_lock: Mutex::new(()),
        })
        .map_err(|_| std::io::Error::other("native quick switch was initialized twice"))?;

    install_dock_menu_hook()
}

/// Refresh both native surfaces after main-window focus or second-instance open.
pub fn refresh() {
    if let Some(controller) = CONTROLLER.get() {
        controller.refresh(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(login: &str, persona: &str, most_recent: bool) -> SteamAccount {
        SteamAccount {
            account_name: login.into(),
            persona_name: persona.into(),
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

    fn account_entries(model: &MenuModel) -> Vec<&ActionEntry> {
        model
            .entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Action(entry) if matches!(entry.action, NativeAction::Switch(_)) => {
                    Some(entry)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn duplicate_personas_are_distinguished_by_login_name() {
        let accounts = vec![
            account("first_login", "Same Persona", false),
            account("second_login", "Same Persona", false),
        ];
        let model = MenuModel::from_accounts(&accounts, true, false);
        let entries = account_entries(&model);

        assert_eq!(entries[0].label, "Same Persona (first_login)");
        assert_eq!(entries[1].label, "Same Persona (second_login)");
        assert_ne!(entries[0].label, entries[1].label);
    }

    #[test]
    fn current_marker_is_truthful_only_while_steam_is_running() {
        let accounts = vec![account("remembered", "Persona", true)];

        let running = MenuModel::from_accounts(&accounts, true, false);
        let running_entry = account_entries(&running)[0];
        assert!(running_entry.checked);
        assert!(!running_entry.enabled);

        let stopped = MenuModel::from_accounts(&accounts, false, false);
        let stopped_entry = account_entries(&stopped)[0];
        assert!(!stopped_entry.checked);
        assert!(stopped_entry.enabled);
    }

    #[test]
    fn previous_most_recent_account_remains_selectable_when_steam_is_stopped() {
        let accounts = vec![
            account("previous", "Previous", true),
            account("other", "Other", false),
        ];
        let model = MenuModel::from_accounts(&accounts, false, false);

        assert!(account_entries(&model).iter().all(|entry| entry.enabled));
        assert!(account_entries(&model).iter().all(|entry| !entry.checked));
    }

    #[test]
    fn busy_dispatch_prevents_a_second_switch() {
        let accounts = vec![
            account("first", "First", false),
            account("second", "Second", false),
        ];
        let model = MenuModel::from_accounts(&accounts, true, false);
        let ids: Vec<_> = account_entries(&model)
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        let mut state = ControllerState { busy: false, model };

        assert_eq!(
            state.begin(&ids[0]),
            Some(NativeAction::Switch("first".into()))
        );
        assert!(state.busy);
        assert_eq!(state.begin(&ids[1]), None);
    }

    #[test]
    fn stable_action_ids_map_exact_accounts_without_parsing_menu_text() {
        let accounts = vec![
            account("login) — misleading", "Persona (other)", false),
            account("exact_target", "Persona (other)", false),
        ];
        let model = MenuModel::from_accounts(&accounts, true, false);
        let entries = account_entries(&model);
        let actions = model.actions();

        assert_eq!(entries[0].id, "native-steam-account-0");
        assert_eq!(entries[1].id, "native-steam-account-1");
        assert_eq!(
            actions.get(&entries[0].id),
            Some(&NativeAction::Switch("login) — misleading".into()))
        );
        assert_eq!(
            actions.get(&entries[1].id),
            Some(&NativeAction::Switch("exact_target".into()))
        );
    }
}
