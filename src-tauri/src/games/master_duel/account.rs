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
/// `steamLogin`, `isLinked`, `hasFiles` to match the TypeScript `MdAccount`
/// interface. `has_files` distinguishes a non-linked profile that still holds
/// its own copy of the cache from one whose `0000` is just the empty skeleton
/// an unlink leaves behind; it is always `false` for linked profiles.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdAccount {
    pub folder_id: String,
    pub account_name: String,
    pub steam_login: String,
    pub is_linked: bool,
    pub has_files: bool,
}

/// A candidate profile to seed a brand-new shared cache from.
///
/// Surfaced when no shared cache exists yet: a profile whose own `0000` is a
/// real directory holding files (not a junction, not empty) can be *moved* into
/// place as the shared cache. `account_name` is the user-chosen label from
/// `accounts.csv` (empty when unlabeled); `size_bytes` is the folder's total
/// file size, used to show "· N.N GB" and to sort largest-first so the user
/// seeds from the most complete cache.
///
/// `serde(rename_all = "camelCase")` maps these to `folderId`, `accountName`,
/// `sizeBytes` for the TypeScript `SeedCandidate` interface.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedCandidate {
    pub folder_id: String,
    pub account_name: String,
    pub size_bytes: u64,
}
