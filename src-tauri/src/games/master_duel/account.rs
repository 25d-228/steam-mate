//! `MdAccount` — one Master Duel account folder as surfaced to the frontend.
//!
//! `folder_id` is the on-disk LocalData subfolder name (a numeric id Konami
//! assigns per Steam account). `account_name` is the user-chosen label stored
//! in `accounts.csv`. `steam_login` is the Steam accountName (login) the
//! profile is assigned to, or empty when unassigned. `is_linked` reflects
//! whether that account's `0000` asset folder is currently a junction into the
//! shared cache.

/// One Master Duel account, ready to serialize to the frontend.
///
/// `serde(rename_all = "camelCase")` maps these to `folderId`, `accountName`,
/// `steamLogin`, `isLinked` to match the TypeScript `MdAccount` interface.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdAccount {
    pub folder_id: String,
    pub account_name: String,
    pub steam_login: String,
    pub is_linked: bool,
}
