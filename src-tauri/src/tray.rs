//! Windows system tray — quick-switch menu mirroring the in-app account list.
//!
//! Windows-only (gated at the crate root like the [`crate::steam`] module): the
//! tray drives the same `steam::switch` flow, which touches the Windows registry
//! and `Steam.exe`. The tray icon stays resident while the window is hidden, so
//! closing the window hides it here rather than quitting — only the tray's
//! "Exit" item quits the app.
//!
//! The menu is rebuilt on demand: at setup we lay it down once with English
//! defaults, and the frontend calls [`tray_refresh`] on mount (and the menu-
//! event handler re-calls it after a switch) with already-localized labels. The
//! [`TrayIcon`] handle plus the last-used labels live in [`TrayState`] (a
//! `Mutex` in managed state) so any of those paths can rebuild it.

use std::sync::Mutex;

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::steam::{switch, vdf};

/// The window label the tray shows/hides/focuses — Tauri's default first window.
const MAIN_WINDOW: &str = "main";

/// Localized labels the tray menu is rendered with.
///
/// `signed_in_text` is the fully-composed header (e.g. "Signed in as 天盃龍") —
/// the frontend composes it so the tray never has to localize. `open_label` and
/// `exit_label` are the two fixed action items. Defaults are English so the
/// setup-time build (before the frontend has mounted) reads sensibly.
#[derive(Clone)]
pub struct TrayLabels {
    pub signed_in_text: String,
    pub open_label: String,
    pub exit_label: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        TrayLabels {
            signed_in_text: "Signed in as —".to_string(),
            open_label: "Open steam-mate".to_string(),
            exit_label: "Exit".to_string(),
        }
    }
}

/// Managed state: the live tray handle plus the labels its menu was last built
/// with, so the menu-event handler can rebuild after a switch without the
/// frontend re-supplying them.
pub struct TrayState {
    pub tray: TrayIcon<Wry>,
    pub labels: TrayLabels,
}

/// Read the remembered Steam accounts, or an empty list on any failure.
///
/// The tray must always render — a missing/unreadable `loginusers.vdf` (Steam
/// not installed, file locked) degrades to "no accounts" rather than erroring,
/// matching how the account list tolerates the same condition.
fn read_accounts() -> Vec<crate::steam::account::SteamAccount> {
    let Ok(path) = crate::steam::paths::loginusers_vdf_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    vdf::parse_loginusers(&text).unwrap_or_default()
}

/// Build the tray menu from the current accounts and the given labels.
///
/// Layout: a disabled header (`signed_in_text`); a separator; one normal item
/// per remembered account labelled `persona（account_name）` with a leading
/// "● " on the currently-active (`most_recent`) account and id
/// `acct:<account_name>`; a separator; `open_label` (id "open"); `exit_label`
/// (id "exit"). The header is disabled so it reads as a caption, not an action.
fn build_menu(app: &AppHandle, labels: &TrayLabels) -> tauri::Result<tauri::menu::Menu<Wry>> {
    let header = MenuItemBuilder::with_id("header", &labels.signed_in_text)
        .enabled(false)
        .build(app)?;
    let sep1 = PredefinedMenuItem::separator(app)?;

    let accounts = read_accounts();
    let mut account_items = Vec::with_capacity(accounts.len());
    for acct in &accounts {
        let dot = if acct.most_recent { "● " } else { "" };
        let label = format!("{dot}{}（{}）", acct.persona_name, acct.account_name);
        let item = MenuItemBuilder::with_id(format!("acct:{}", acct.account_name), label)
            .build(app)?;
        account_items.push(item);
    }

    let sep2 = PredefinedMenuItem::separator(app)?;
    let open = MenuItemBuilder::with_id("open", &labels.open_label).build(app)?;
    let exit = MenuItemBuilder::with_id("exit", &labels.exit_label).build(app)?;

    let mut builder = MenuBuilder::new(app).item(&header).item(&sep1);
    for item in &account_items {
        builder = builder.item(item);
    }
    builder.item(&sep2).item(&open).item(&exit).build()
}

/// Show, unminimize, and focus the main window (used by left-click and "Open").
fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Handle a tray menu selection by item id.
///
/// `acct:<name>` runs the account switch (online) on a blocking thread, then on
/// success emits `accounts-changed` to the main window and rebuilds the tray
/// menu (reusing the last labels) so the active "●" moves. "open" focuses the
/// window; "exit" quits the whole app (the only path that does, since closing
/// the window only hides it).
fn on_menu_event(app: &AppHandle, id: &str) {
    if let Some(account_name) = id.strip_prefix("acct:") {
        let app = app.clone();
        let account_name = account_name.to_string();
        // Switching kills + relaunches Steam — keep it off the UI/event thread.
        std::thread::spawn(move || {
            let result = switch::switch_account(&account_name, false);
            // Emit and rebuild on BOTH outcomes: even a failed switch may have
            // changed real state (Steam killed, file half-flow recovered), and
            // a silent failure would leave the old "●" lying about what
            // happened. The frontend re-reads the truth either way.
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.emit("accounts-changed", ());
                if let Err(e) = &result {
                    let _ = window.emit("switch-error", e.to_string());
                }
            }
            refresh_with_last_labels(&app);
        });
        return;
    }
    match id {
        "open" => focus_main_window(app),
        "exit" => app.exit(0),
        _ => {}
    }
}

/// Rebuild the tray menu using whatever labels are currently in state.
///
/// Used after a tray-initiated switch (the frontend isn't involved, so it can't
/// re-supply labels). The new menu is swapped in via [`TrayIcon::set_menu`].
fn refresh_with_last_labels(app: &AppHandle) {
    let state = app.state::<Mutex<TrayState>>();
    let labels = {
        let guard = state.lock().unwrap();
        guard.labels.clone()
    };
    if let Ok(menu) = build_menu(app, &labels) {
        let guard = state.lock().unwrap();
        let _ = guard.tray.set_menu(Some(menu));
    }
}

/// Build the tray at app setup with English defaults and store it in state.
///
/// Called from the Tauri `setup` hook. Uses the app's default window icon for
/// the tray image, wires the menu-event handler and a left-click handler that
/// focuses the window, and manages a [`TrayState`] holding the handle + the
/// default labels. The frontend replaces the labels via [`tray_refresh`] on
/// mount once its language is known.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let labels = TrayLabels::default();
    let menu = build_menu(app, &labels)?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("steam-mate")
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            // Left button release shows the window; the menu opens on right-click.
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                focus_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;

    app.manage(Mutex::new(TrayState { tray, labels }));
    Ok(())
}

/// Rebuild the tray menu with frontend-supplied, already-localized labels.
///
/// `signed_in_text` is the composed header (e.g. "Signed in as 天盃龍"). The
/// labels are stored so a later tray-initiated switch can rebuild with the same
/// language, then the menu is rebuilt from the current account list and swapped
/// in. Called by the frontend on mount and whenever the language changes.
#[tauri::command]
pub async fn tray_refresh(
    app: AppHandle,
    signed_in_text: String,
    open_label: String,
    exit_label: String,
) -> Result<(), String> {
    let labels = TrayLabels {
        signed_in_text,
        open_label,
        exit_label,
    };
    let menu = build_menu(&app, &labels).map_err(|e| e.to_string())?;

    let state = app.state::<Mutex<TrayState>>();
    let guard = state.lock().map_err(|e| e.to_string())?;
    guard.tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    drop(guard);
    // Persist the labels for tray-initiated rebuilds after a quick-switch.
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.labels = labels;
    Ok(())
}
