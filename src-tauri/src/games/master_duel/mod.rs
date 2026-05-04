//! Yu-Gi-Oh! Master Duel — link/unlink per-account asset folders to a
//! shared cache via NTFS junctions. Windows-only (entire module is
//! gated by `#[cfg(windows)]` in `games/mod.rs`).

pub mod account;
pub mod commands;
pub mod csv;
pub mod link;
pub mod paths;
