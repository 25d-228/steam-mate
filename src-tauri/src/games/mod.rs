//! Games module — per-game features. Each game is conditionally compiled
//! per its supported OS. Master Duel is Windows-only.

#[cfg(windows)]
pub mod master_duel;
