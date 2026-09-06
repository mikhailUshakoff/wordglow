use std::collections::HashMap;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
use tauri::State;

use crate::dto::{
    BookDto, DictionaryEntryDto, LessonAudioDto, LessonDetailDto, LessonDto, LessonStatus, WordTimepointDto,
};

pub struct DbState(pub Mutex<Connection>);

/// Ollama connection settings, loaded once at startup from the environment
/// (or a `.env` file) via `envy`. Both fields have defaults, so no env vars
/// are required for a local Ollama server.
#[derive(serde::Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "OllamaConfig::default_url")]
    pub ollama_url: String,
    #[serde(default = "OllamaConfig::default_model")]
    pub ollama_model: String,
    /// Bearer token for Ollama's hosted cloud API. Not needed for a local
    /// Ollama server.
    pub ollama_api_key: Option<String>,
}

impl OllamaConfig {
    fn default_url() -> String {
        "https://ollama.com".to_string()
    }

    fn default_model() -> String {
        "gpt-oss:120b".to_string()
    }
}

fn book_to_dto(book: wordglow_core::models::Book) -> BookDto {
    BookDto {
        id: book.id,
        title: book.title,
        author: book.author,
        language: book.language,
    }
}

fn lesson_status_to_dto(status: wordglow_core::models::LessonStatus) -> LessonStatus {
    match status {
        wordglow_core::models::LessonStatus::NotStarted => LessonStatus::NotStarted,
        wordglow_core::models::LessonStatus::InProgress => LessonStatus::InProgress,
        wordglow_core::models::LessonStatus::Completed => LessonStatus::Completed,
    }
}

fn lesson_status_to_core(status: LessonStatus) -> wordglow_core::models::LessonStatus {
    match status {
        LessonStatus::NotStarted => wordglow_core::models::LessonStatus::NotStarted,
        LessonStatus::InProgress => wordglow_core::models::LessonStatus::InProgress,
        LessonStatus::Completed => wordglow_core::models::LessonStatus::Completed,
    }
}

fn dictionary_entry_to_dto(entry: wordglow_core::models::DictionaryEntry) -> DictionaryEntryDto {
    DictionaryEntryDto {
        id: entry.id,
        word: entry.word,
        translation: entry.translation,
        lesson_id: entry.lesson_id,
        created_at: entry.created_at,
    }
}

fn lesson_to_dto(lesson: wordglow_core::models::Lesson) -> LessonDto {
    LessonDto {
        id: lesson.id,
        book_id: lesson.book_id,
        order_index: lesson.order_index,
        title: lesson.title,
    }
}

#[tauri::command]
pub fn list_books(state: State<DbState>) -> Result<Vec<BookDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::books::list_all(&conn)
        .map(|books| books.into_iter().map(book_to_dto).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_lessons(state: State<DbState>, book_id: String) -> Result<Vec<LessonDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::lessons::list_for_book(&conn, &book_id)
        .map(|lessons| lessons.into_iter().map(lesson_to_dto).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_lesson(state: State<DbState>, lesson_id: String) -> Result<LessonDetailDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let lesson = wordglow_core::lessons::get(&conn, &lesson_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no lesson with id {lesson_id}"))?;
    Ok(LessonDetailDto {
        id: lesson.id,
        book_id: lesson.book_id,
        order_index: lesson.order_index,
        title: lesson.title,
        text: lesson.text,
    })
}

#[tauri::command]
pub fn get_lesson_audio(state: State<DbState>, lesson_id: String) -> Result<Option<LessonAudioDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    let lesson = wordglow_core::lessons::get(&conn, &lesson_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no lesson with id {lesson_id}"))?;

    let Some(audio) = wordglow_core::tts::get_audio(&conn, &lesson_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };

    let audio_dir = wordglow_core::paths::audio_dir().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(audio_dir.join(&audio.audio_path)).map_err(|e| e.to_string())?;

    Ok(Some(LessonAudioDto {
        audio_base64: STANDARD.encode(bytes),
        mime: "audio/mpeg".to_string(),
        voice: audio.voice,
        word_timepoints: audio
            .word_timepoints
            .into_iter()
            .map(|tp| WordTimepointDto {
                word: tp.word,
                start_ms: tp.start_ms,
                end_ms: tp.end_ms,
            })
            .collect(),
        generated_at: audio.generated_at,
        stale: audio.text_hash_at_gen != lesson.text_hash,
    }))
}

#[tauri::command]
pub fn get_lesson_statuses(state: State<DbState>) -> Result<HashMap<String, LessonStatus>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::lessons::list_all_statuses(&conn)
        .map(|map| map.into_iter().map(|(id, s)| (id, lesson_status_to_dto(s))).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_lesson_status(state: State<DbState>, lesson_id: String, status: LessonStatus) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::lessons::set_status(&conn, &lesson_id, lesson_status_to_core(status)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_word(state: State<DbState>, word: String, lesson_id: Option<String>) -> Result<DictionaryEntryDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::dictionary::add_word(&conn, &word, lesson_id.as_deref())
        .map(dictionary_entry_to_dto)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_word(state: State<DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::dictionary::remove_word(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_dictionary(state: State<DbState>) -> Result<Vec<DictionaryEntryDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::dictionary::list_all(&conn)
        .map(|entries| entries.into_iter().map(dictionary_entry_to_dto).collect())
        .map_err(|e| e.to_string())
}

/// Translate `word` to Russian via Ollama. Reuses a cached translation on
/// the word's dictionary entry if it has one; otherwise calls Ollama and,
/// if the word is on the list, caches the result for next time.
#[tauri::command]
pub fn translate_word(state: State<DbState>, ollama: State<OllamaConfig>, word: String) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    if let Some(cached) = wordglow_core::dictionary::get_by_word(&conn, &word)
        .map_err(|e| e.to_string())?
        .and_then(|entry| entry.translation)
    {
        return Ok(cached);
    }

    let translation = wordglow_core::ollama::translate_word(
        &ollama.ollama_url,
        ollama.ollama_api_key.as_deref(),
        &ollama.ollama_model,
        &word,
    )
    .map_err(|e| e.to_string())?;

    wordglow_core::dictionary::set_translation(&conn, &word, &translation).map_err(|e| e.to_string())?;

    Ok(translation)
}
