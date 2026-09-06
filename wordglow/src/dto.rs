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
