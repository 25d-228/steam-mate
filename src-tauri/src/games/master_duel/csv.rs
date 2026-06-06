//! Read/write `accounts.csv` — the `folder_id` ↔ `account_name` ↔ `steam_login`
//! mapping.
//!
//! UTF-8 throughout so CJK labels (e.g. `烙印`, `キラーチューン`) round-trip
//! intact. The canonical on-disk header is the 3-column
//! `folder_id,account_name,steam_login`; we still *read* the older 2-column
//! `folder_id,account_name` form and the legacy 3-column
//! `folder_id,account_type,account_name` form (the `account_type` concept is
//! gone), and rewrite either into the new 3-column form on the next save.
//!
//! Every column is resolved by *header name*, never by position — that is what
//! lets the legacy `account_type` column be ignored without shifting
//! `account_name`, and lets a 2-column file (no `steam_login`) read back with an
//! empty login rather than a garbled one.

use std::path::Path;

use crate::error::{AppError, AppResult};

/// New, canonical CSV header.
const HEADER: [&str; 3] = ["folder_id", "account_name", "steam_login"];

/// One account row: `(folder_id, account_name, steam_login)`.
///
/// `steam_login` is the Steam *accountName* (login) this profile belongs to, or
/// the empty string when unassigned.
pub type Row = (String, String, String);

/// Map a `csv::Error` into the crate-wide `AppError`.
///
/// CSV failures are IO/parse problems on a single file; there's no dedicated
/// variant (per the single-`AppError` house rule), so they fold into
/// [`AppError::Io`] with the underlying message preserved.
fn csv_err(e: csv::Error) -> AppError {
    AppError::Io(e.to_string())
}

/// Read `accounts.csv` into `(folder_id, account_name, steam_login)` rows.
///
/// Returns an empty vec if the file does not exist. Columns are resolved by
/// *header name*, so every historical layout parses correctly:
/// - new 3-column `folder_id,account_name,steam_login`,
/// - old 2-column `folder_id,account_name` (login reads back empty),
/// - legacy 3-column `folder_id,account_type,account_name` (`account_type`
///   ignored, `account_name` still found by name, login empty).
///
/// A missing cell (e.g. no `steam_login` column, or a short flexible row) yields
/// an empty string rather than dropping or shifting the row.
pub fn read_accounts(path: &Path) -> AppResult<Vec<Row>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(csv_err)?;

    let headers = rdr.headers().map_err(csv_err)?.clone();
    let id_idx = headers.iter().position(|h| h == "folder_id");
    let name_idx = headers.iter().position(|h| h == "account_name");
    let login_idx = headers.iter().position(|h| h == "steam_login");

    let cell = |record: &csv::StringRecord, idx: Option<usize>| -> String {
        idx.and_then(|i| record.get(i)).unwrap_or("").to_string()
    };

    let mut out = Vec::new();
    for record in rdr.records() {
        let record = record.map_err(csv_err)?;
        let folder_id = cell(&record, id_idx);
        // A row with no folder_id is meaningless (nothing to key on); skip it
        // rather than carry an empty id that can never match a real folder.
        if folder_id.is_empty() {
            continue;
        }
        let account_name = cell(&record, name_idx);
        let steam_login = cell(&record, login_idx);
        out.push((folder_id, account_name, steam_login));
    }
    Ok(out)
}

/// Write the full account list back to `path` atomically.
///
/// Serializes to a sibling `*.tmp` file then renames over the target, so a
/// crash mid-write never leaves a truncated `accounts.csv`. The `csv` writer
/// quotes any field containing a comma/quote/newline, so logins or names with
/// commas round-trip without corrupting later columns.
fn write_all(path: &Path, rows: &[Row]) -> AppResult<()> {
    let tmp = path.with_extension("csv.tmp");
    {
        let mut wtr = csv::WriterBuilder::new()
            .from_path(&tmp)
            .map_err(csv_err)?;
        wtr.write_record(HEADER).map_err(csv_err)?;
        for (folder_id, account_name, steam_login) in rows {
            wtr.write_record([
                folder_id.as_str(),
                account_name.as_str(),
                steam_login.as_str(),
            ])
            .map_err(csv_err)?;
        }
        wtr.flush()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Insert or update the label for `folder_id`, then rewrite the whole file.
///
/// If a row for `folder_id` already exists only its `account_name` is replaced —
/// the existing `steam_login` is preserved, so renaming a profile never drops
/// its Steam assignment. A brand-new id is appended with an empty login. Always
/// writes the 3-column header.
pub fn upsert_account(path: &Path, folder_id: &str, name: &str) -> AppResult<()> {
    let mut rows = read_accounts(path)?;
    match rows.iter_mut().find(|(id, _, _)| id == folder_id) {
        Some(row) => row.1 = name.to_string(),
        None => rows.push((folder_id.to_string(), name.to_string(), String::new())),
    }
    write_all(path, &rows)
}

/// Set (or clear, with an empty string) the Steam login for `folder_id`.
///
/// Updates only the `steam_login` column of an existing row, preserving its
/// `account_name`; a brand-new id is appended with an empty name. Always writes
/// the 3-column header.
pub fn set_login(path: &Path, folder_id: &str, login: &str) -> AppResult<()> {
    let mut rows = read_accounts(path)?;
    match rows.iter_mut().find(|(id, _, _)| id == folder_id) {
        Some(row) => row.2 = login.to_string(),
        None => rows.push((folder_id.to_string(), String::new(), login.to_string())),
    }
    write_all(path, &rows)
}

/// Remove the row for `folder_id` (if present), then rewrite the whole file.
///
/// A no-op (still rewrites in 3-column form) if no such row exists.
pub fn remove_account(path: &Path, folder_id: &str) -> AppResult<()> {
    let mut rows = read_accounts(path)?;
    rows.retain(|(id, _, _)| id != folder_id);
    write_all(path, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("master_duel")
            .join(name)
    }

    #[test]
    fn missing_file_is_empty() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("master_duel")
            .join("does_not_exist.csv");
        assert!(read_accounts(&path).unwrap().is_empty());
    }

    #[test]
    fn reads_three_column_with_cjk_and_login() {
        let rows = read_accounts(&fixture("accounts.csv")).unwrap();
        let find = |id: &str| rows.iter().find(|(fid, _, _)| fid == id).cloned();
        assert_eq!(
            find("0000000000000001"),
            Some(("0000000000000001".into(), "烙印".into(), "brand_acc".into()))
        );
        assert_eq!(
            find("0000000000000002"),
            Some(("0000000000000002".into(), "キラーチューン".into(), "".into()))
        );
        assert_eq!(
            find("0000000000000003"),
            Some(("0000000000000003".into(), "Plushie".into(), "plushie_login".into()))
        );
    }

    #[test]
    fn reads_two_column_login_empty() {
        let rows = read_accounts(&fixture("accounts_2col.csv")).unwrap();
        let find = |id: &str| rows.iter().find(|(fid, _, _)| fid == id).cloned();
        // No steam_login column ⇒ empty login, name still resolves by header.
        assert_eq!(
            find("0000000000000020"),
            Some(("0000000000000020".into(), "幻奏".into(), "".into()))
        );
    }

    #[test]
    fn reads_legacy_three_column_header() {
        let rows = read_accounts(&fixture("accounts_legacy.csv")).unwrap();
        let find = |id: &str| {
            rows.iter()
                .find(|(fid, _, _)| fid == id)
                .map(|(_, n, l)| (n.clone(), l.clone()))
        };
        // account_type column is ignored; name still resolves by header, and
        // there is no steam_login column so the login is empty (NOT the
        // account_type value).
        assert_eq!(find("0000000000000010"), Some(("鬼ガエル".into(), "".into())));
        assert_eq!(find("0000000000000011"), Some(("サブ".into(), "".into())));
    }

    #[test]
    fn quoted_comma_in_name_and_login_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.csv");
        upsert_account(&path, "x", "a, b, c").unwrap();
        set_login(&path, "x", "lo,gin").unwrap();
        let rows = read_accounts(&path).unwrap();
        assert_eq!(
            rows,
            vec![("x".to_string(), "a, b, c".to_string(), "lo,gin".to_string())]
        );
    }

    #[test]
    fn upsert_inserts_then_updates_and_preserves_login() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.csv");

        upsert_account(&path, "abc", "斬機").unwrap();
        assert_eq!(
            read_accounts(&path).unwrap(),
            vec![("abc".to_string(), "斬機".to_string(), "".to_string())]
        );

        // Assign a login, then RENAME — the login must survive the rename.
        set_login(&path, "abc", "konami_login").unwrap();
        upsert_account(&path, "abc", "ティアラメンツ").unwrap();
        let rows = read_accounts(&path).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "ティアラメンツ");
        assert_eq!(rows[0].2, "konami_login");

        // Append a new id (empty login).
        upsert_account(&path, "def", "Snake-Eye").unwrap();
        assert_eq!(read_accounts(&path).unwrap().len(), 2);
    }

    #[test]
    fn set_login_preserves_name_and_can_clear() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.csv");
        upsert_account(&path, "id1", "炎王").unwrap();

        set_login(&path, "id1", "login_a").unwrap();
        let rows = read_accounts(&path).unwrap();
        assert_eq!(rows[0], ("id1".into(), "炎王".into(), "login_a".into()));

        // Reassign keeps the name.
        set_login(&path, "id1", "login_b").unwrap();
        assert_eq!(read_accounts(&path).unwrap()[0].1, "炎王");
        // Clear with empty string keeps the name, drops the login.
        set_login(&path, "id1", "").unwrap();
        let rows = read_accounts(&path).unwrap();
        assert_eq!(rows[0], ("id1".into(), "炎王".into(), "".into()));
    }

    #[test]
    fn set_login_on_new_id_appends_with_empty_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.csv");
        set_login(&path, "fresh", "only_login").unwrap();
        assert_eq!(
            read_accounts(&path).unwrap(),
            vec![("fresh".to_string(), "".to_string(), "only_login".to_string())]
        );
    }

    #[test]
    fn remove_drops_row_and_keeps_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.csv");
        upsert_account(&path, "a", "一").unwrap();
        upsert_account(&path, "b", "二").unwrap();

        remove_account(&path, "a").unwrap();
        assert_eq!(
            read_accounts(&path).unwrap(),
            vec![("b".to_string(), "二".to_string(), "".to_string())]
        );

        // Removing a non-existent id is a no-op.
        remove_account(&path, "zzz").unwrap();
        assert_eq!(read_accounts(&path).unwrap().len(), 1);
    }

    #[test]
    fn legacy_file_is_rewritten_as_three_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.csv");
        std::fs::copy(fixture("accounts_legacy.csv"), &path).unwrap();

        // Any write rewrites the header to the new 3-column form, and the legacy
        // account_type value never leaks into steam_login.
        upsert_account(&path, "0000000000000010", "炎王").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let first_line = text.lines().next().unwrap();
        assert_eq!(first_line, "folder_id,account_name,steam_login");
        let rows = read_accounts(&path).unwrap();
        let r = rows.iter().find(|(id, _, _)| id == "0000000000000010").unwrap();
        assert_eq!(r.2, ""); // login empty, NOT "main"
    }
}
