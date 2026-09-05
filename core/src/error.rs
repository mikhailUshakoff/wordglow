use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("tts error: {0}")]
    Tts(String),

    #[error("could not determine data directory")]
    NoDataDir,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("ambiguous: {0}")]
    Ambiguous(String),

    #[error("invalid reorder: {0}")]
    InvalidReorder(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
