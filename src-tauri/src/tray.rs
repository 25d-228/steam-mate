//! Windows system tray — icon only; the menu is a rich webview popup.
//!
//! Windows-only (gated at the crate root like the [`crate::steam`] module): the
//! tray icon stays resident while the main window is hidden, so closing the
//! window hides it here rather than quitting — only the popup's "Exit" item
//! (which invokes [`app_exit`]) quits the app.
//!
//! There is no native menu. Right-clicking the tray icon positions and shows a
//! borderless always-on-top "tray" webview window that renders the colorful
//! account list (avatars, the signed-in dot, Open/Exit) itself; left-clicking
//! restores the main window.

use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Wry};

/// The window label the tray restores/focuses — Tauri's default first window.
const MAIN_WINDOW: &str = "main";

/// The borderless popup window the tray opens on right-click. Declared in
/// `tauri.conf.json` (hidden, decorationless, always-on-top); it renders the
/// account list and quick-switch UI itself.
const TRAY_WINDOW: &str = "tray";

/// Gap in physical pixels between the click point and the popup's bottom edge.
const POPUP_GAP: f64 = 8.0;

/// Show, unminimize, and focus the main window (used by the left-click handler).
fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Position the popup window above-and-left of the click point, then show it.
///
/// The popup anchors so its bottom-right corner sits near the tray icon: with
/// its outer size `(w, h)`, top-left goes to `(click.x - w, click.y - h - gap)`,
/// clamped to the bounds of the monitor hosting the click. Clamping to the
/// monitor — not to `(0, 0)` — matters on multi-monitor layouts: a monitor
/// placed left of or above the primary lives at NEGATIVE virtual-desktop
/// coordinates, where a zero clamp would teleport the popup to the primary.
/// Before showing we emit `tray-popup-will-show` so the webview re-reads its
/// data (running state, accounts, active dot) for the freshly-opened menu.
fn show_popup(app: &AppHandle, click: PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window(TRAY_WINDOW) else {
        return;
    };
    if let Ok(size) = window.outer_size() {
        let mut x = click.x - size.width as f64;
        let mut y = click.y - size.height as f64 - POPUP_GAP;
        if let Ok(Some(monitor)) = app.monitor_from_point(click.x, click.y) {
            let min_x = monitor.position().x as f64;
            let min_y = monitor.position().y as f64;
            let max_x = (min_x + monitor.size().width as f64 - size.width as f64).max(min_x);
            let max_y = (min_y + monitor.size().height as f64 - size.height as f64).max(min_y);
            x = x.clamp(min_x, max_x);
            y = y.clamp(min_y, max_y);
        } else {
            // Unknown monitor: at least keep the primary's origin in bounds.
            x = x.max(0.0);
            y = y.max(0.0);
        }
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.emit("tray-popup-will-show", ());
    let _ = window.show();
    let _ = window.set_focus();
}

/// Build the tray icon at app setup (no menu) and store nothing — it's resident.
///
/// Called from the Tauri `setup` hook. Uses the app's default window icon for
/// the tray image and a "steam-mate" tooltip, with NO `.menu()`: the popup
/// webview is the menu. Left-click (button up) restores the main window;
/// right-click (button up) positions and shows the popup.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::new()
        .tooltip("steam-mate")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state: tauri::tray::MouseButtonState::Up,
                position,
                ..
            } = event
            {
                let app = tray.app_handle();
                match button {
                    // Left release restores the main window.
                    tauri::tray::MouseButton::Left => focus_main_window(app),
                    // Right release opens the popup menu at the click point.
                    tauri::tray::MouseButton::Right => show_popup(app, position),
                    _ => {}
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let _tray: TrayIcon<Wry> = builder.build(app)?;
    Ok(())
}

/// Quit the whole app — the only path that does.
///
/// Invoked by the popup's "Exit" item. Closing the main window only hides it to
/// the tray (see the `CloseRequested` handler in `lib.rs`), so this command is
/// the sole way to actually exit.
#[tauri::command]
pub async fn app_exit(app: AppHandle) {
    app.exit(0);
}
