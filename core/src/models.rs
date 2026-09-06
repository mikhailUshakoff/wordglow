use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub book_id: String,
    pub order_index: i64,
    pub title: String,
    pub text: String,
    pub text_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTimepoint {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonAudio {
    pub lesson_id: String,
    pub audio_path: String,
    pub voice: String,
    pub word_timepoints: Vec<WordTimepoint>,
    pub generated_at: String,
    pub text_hash_at_gen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub id: i64,
    pub word: String,
    pub translation: Option<String>,
    pub lesson_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub lesson_id: String,
    pub order_index: i64,
    pub question_text: String,
    pub created_at: String,
}
