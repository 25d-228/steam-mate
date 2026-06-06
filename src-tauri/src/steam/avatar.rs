//! Fetch and cache Steam profile avatars as `data:` URIs.
//!
//! Avatars are a nicety, never a requirement: any failure (bad id, no network,
//! malformed XML, oversized image) degrades to `Ok(None)` so the account list
//! still renders.

use std::fs;
use std::io::Read;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::error::{AppError, AppResult};

/// Cap on the avatar image we'll cache, in bytes (1 MiB). Steam's "medium"
/// avatars are tiny; this only guards against a hostile/huge response.
const MAX_AVATAR_BYTES: u64 = 1024 * 1024;

/// Extract the first `avatarMedium` URL from Steam profile XML.
///
/// Steam embeds it as `<avatarMedium><![CDATA[https://...]]></avatarMedium>`.
/// We slice between the literal `<![CDATA[` opener and the `]]>` closer with
/// plain string search (no regex / XML parser — the format is fixed). Returns
/// `None` if either marker is absent.
fn extract_avatar_url(xml: &str) -> Option<&str> {
    const OPEN: &str = "<avatarMedium><![CDATA[";
    const CLOSE: &str = "]]>";
    let start = xml.find(OPEN)? + OPEN.len();
    let rest = &xml[start..];
    let end = rest.find(CLOSE)?;
    Some(&rest[..end])
}

/// Return a Steam account's avatar as a `data:image/jpeg;base64,...` URI.
///
/// `steam_id64` must be all ASCII digits, else we return `Ok(None)` (we never
/// hit the network for a malformed id). Avatars are cached under
/// `<data-dir>/steam-mate/avatars/<id>.jpg`; a cache hit is read and encoded
/// directly. On a miss we fetch the profile XML from steamcommunity.com, pull
/// the `avatarMedium` URL, download the image (capped at 1 MiB), cache it, and
/// return the URI. Any network/parse/size failure yields `Ok(None)` — fetching
/// avatars must never break the caller.
pub fn get_avatar(steam_id64: &str) -> AppResult<Option<String>> {
    if steam_id64.is_empty() || !steam_id64.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }

    let cache_dir = match dirs::data_dir() {
        Some(d) => d.join("steam-mate").join("avatars"),
        None => return Ok(None),
    };
    fs::create_dir_all(&cache_dir).map_err(|e| AppError::Io(e.to_string()))?;
    let cache_file = cache_dir.join(format!("{steam_id64}.jpg"));

    // Cache hit: read the bytes and encode them.
    if cache_file.exists() {
        let bytes = fs::read(&cache_file).map_err(|e| AppError::Io(e.to_string()))?;
        return Ok(Some(to_data_uri(&bytes)));
    }

    // Cache miss: fetch profile XML. Any error -> Ok(None).
    let xml_url = format!("https://steamcommunity.com/profiles/{steam_id64}?xml=1");
    let xml = match ureq::get(&xml_url)
        .timeout(Duration::from_secs(5))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
    {
        Some(x) => x,
        None => return Ok(None),
    };

    let avatar_url = match extract_avatar_url(&xml) {
        Some(u) => u.to_string(),
        None => return Ok(None),
    };

    // Download the image, reading at most MAX_AVATAR_BYTES + 1 to detect overflow.
    let resp = match ureq::get(&avatar_url).timeout(Duration::from_secs(5)).call() {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    let mut bytes = Vec::new();
    if resp
        .into_reader()
        .take(MAX_AVATAR_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(None);
    }
    if bytes.is_empty() || bytes.len() as u64 > MAX_AVATAR_BYTES {
        return Ok(None);
    }

    // Best-effort cache write; a failure here shouldn't lose the avatar we have.
    let _ = fs::write(&cache_file, &bytes);
    Ok(Some(to_data_uri(&bytes)))
}

/// Base64-encode JPEG bytes into a `data:image/jpeg;base64,...` URI.
fn to_data_uri(bytes: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_avatar_url_from_cdata() {
        let xml = r#"<profile><avatarMedium><![CDATA[https://avatars.example/abc_medium.jpg]]></avatarMedium></profile>"#;
        assert_eq!(
            extract_avatar_url(xml),
            Some("https://avatars.example/abc_medium.jpg")
        );
    }

    #[test]
    fn extracts_first_when_multiple_present() {
        let xml = "<x><avatarMedium><![CDATA[first]]></avatarMedium><avatarMedium><![CDATA[second]]></avatarMedium></x>";
        assert_eq!(extract_avatar_url(xml), Some("first"));
    }

    #[test]
    fn missing_open_marker_yields_none() {
        assert_eq!(extract_avatar_url("<profile>no avatar here</profile>"), None);
    }

    #[test]
    fn missing_close_marker_yields_none() {
        assert_eq!(
            extract_avatar_url("<avatarMedium><![CDATA[unterminated"),
            None
        );
    }
}
