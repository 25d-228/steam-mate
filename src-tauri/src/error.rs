//! Crate-wide error type. Every Tauri command returns `AppResult<T>`.
//! The `#[serde(tag = "kind", content = "msg")]` attribute makes the
//! JSON sent to the frontend look like `{ "kind": "VdfParse", "msg": "..." }`,
//! which the TS side can pattern-match on `kind`.

#[derive(thiserror::Error, Debug, serde::Serialize)]
#[serde(tag = "kind", content = "msg")]
pub enum AppError {
    #[error("steam not installed")]              SteamNotInstalled,
    #[error("vdf parse failed: {0}")]            VdfParse(String),
    #[error("registry write failed: {0}")]       RegistryWrite(String),
    #[error("io error: {0}")]                    Io(String),
    #[error("game not installed: {0}")]          GameNotInstalled(&'static str),
    #[error("game running: {0}")]                GameRunning(&'static str),
    #[error("junction failed: {0}")]             JunctionFailed(String),
    #[error("account not found: {0}")]           AccountNotFound(String),
    #[error("process kill failed: {0}")]         ProcessKillFailed(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
