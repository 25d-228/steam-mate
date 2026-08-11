//! Steam account management shared by Windows and macOS.
//!
//! VDF parsing, avatar loading, file writes, and account orchestration stay in
//! this module. Only host integration lives behind [`platform`].

pub mod account;
pub mod avatar;
pub mod commands;
pub mod file;
pub mod platform;
pub mod switch;
pub mod vdf;
