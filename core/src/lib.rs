pub mod books;
pub mod db;
pub mod dictionary;
pub mod error;
pub mod hash;
pub mod lessons;
pub mod migrations;
pub mod models;
pub mod ollama;
pub mod paths;
pub mod questions;
pub mod tts;

pub use error::{CoreError, Result};

/// Current UTC time as an ISO 8601 string, for `created_at`/`updated_at` columns.
pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}
