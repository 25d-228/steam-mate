//! Shared file operations used by Steam's VDF stores.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::error::{AppError, AppResult};

/// Replace `path` atomically with `text` using a sibling temporary file.
pub fn atomic_write(path: &Path, text: &str) -> AppResult<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".steam-mate.tmp");
    let tmp = std::path::PathBuf::from(tmp);

    let mut file = fs::File::create(&tmp).map_err(|e| AppError::Io(e.to_string()))?;
    file.write_all(text.as_bytes())
        .map_err(|e| AppError::Io(e.to_string()))?;
    file.sync_all().map_err(|e| AppError::Io(e.to_string()))?;
    drop(file);

    fs::rename(&tmp, path).map_err(|e| AppError::Io(e.to_string()))
}
