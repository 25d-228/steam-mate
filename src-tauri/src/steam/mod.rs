//! Steam module — cross-platform account switching.
//! Exposes `commands` (the `#[tauri::command]` entry points) plus
//! the internals they orchestrate.

pub mod account;
pub mod commands;
pub mod paths;
pub mod process;
pub mod registry;
pub mod switch;
pub mod vdf;
