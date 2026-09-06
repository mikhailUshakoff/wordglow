use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookDto {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonDto {
    pub id: String,
    pub book_id: String,
    pub order_index: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonDetailDto {
    pub id: String,
    pub book_id: String,
    pub order_index: i64,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTimepointDto {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonAudioDto {
    /// Base64-encoded audio bytes (small enough per-lesson to ship over IPC
    /// without a custom asset protocol).
    pub audio_base64: String,
    pub mime: String,
    pub voice: String,
    pub word_timepoints: Vec<WordTimepointDto>,
    pub generated_at: String,
    /// True when the lesson's text has changed since this audio was generated.
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionDto {
    pub id: String,
    pub order_index: i64,
    pub question_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntryDto {
    pub id: i64,
    pub word: String,
    pub translation: Option<String>,
    pub lesson_id: Option<String>,
    pub created_at: String,
}

/// Per-lesson reading status. Session-only for now: nothing in the schema
/// persists this yet, so it lives in the frontend's own state and resets
/// every app launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LessonStatus {
    #[default]
    NotStarted,
    InProgress,
    Completed,
}

impl LessonStatus {
    pub fn label(self) -> &'static str {
        match self {
            LessonStatus::NotStarted => "Not started",
            LessonStatus::InProgress => "In progress",
            LessonStatus::Completed => "Completed",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            LessonStatus::NotStarted => "status status-not-started",
            LessonStatus::InProgress => "status status-in-progress",
            LessonStatus::Completed => "status status-completed",
        }
    }
}
