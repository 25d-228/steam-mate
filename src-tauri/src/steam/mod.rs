//! Steam module — account switching. Windows-only: it reads the Windows
//! registry (`winreg`) and drives `Steam.exe`, so the whole module is gated to
//! `#[cfg(windows)]` at the crate root.
//! Exposes `commands` (the `#[tauri::command]` entry points) plus
//! the internals they orchestrate.

pub mod account;
pub mod avatar;
pub mod commands;
pub mod paths;
pub mod process;
pub mod registry;
pub mod switch;
pub mod vdf;
