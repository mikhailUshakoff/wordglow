use std::collections::HashMap;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
use tauri::State;

use crate::dto::{
    BookDto, DictionaryEntryDto, ImportSummaryDto, LessonAudioDto, LessonDetailDto, LessonDto, LessonStatus,
    QuestionDto, WordTimepointDto,
};

pub struct DbState(pub Mutex<Connection>);

/// MyMemory translation settings, loaded once at startup from the
/// environment (or a `.env` file) via `envy`. No env vars are required.
#[derive(serde::Deserialize)]
pub struct TranslateConfig {
    /// Fallback ISO 639-1 source language (`TRANSLATE_SOURCE_LANG`), used
    /// only when the word's lesson/book has no language set.
    #[serde(default = "TranslateConfig::default_source_lang")]
    pub translate_source_lang: String,
    /// Optional email sent to MyMemory (`MYMEMORY_EMAIL`); raises the free
    /// daily quota from ~5k to ~50k chars.
    pub mymemory_email: Option<String>,
}

impl TranslateConfig {
    fn default_source_lang() -> String {
        "en".to_string()
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
pub fn get_lesson_questions(state: State<DbState>, lesson_id: String) -> Result<Vec<QuestionDto>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    wordglow_core::questions::list_for_lesson(&conn, &lesson_id)
        .map(|questions| {
            questions
                .into_iter()
                .map(|q| QuestionDto { id: q.id, order_index: q.order_index, question_text: q.question_text })
                .collect()
        })
        .map_err(|e| e.to_string())
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

/// Import a content package (a .zip produced by `teacher-cli export`),
/// base64-encoded since Tauri IPC carries JSON. Upserts books/lessons by
/// UUID and never touches a lesson's reading `status`.
#[tauri::command]
pub fn import_package(state: State<DbState>, data_base64: String) -> Result<ImportSummaryDto, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let bytes = STANDARD.decode(data_base64).map_err(|e| e.to_string())?;
    let summary = wordglow_core::package::import_package(&conn, std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    Ok(ImportSummaryDto {
        books: summary.books,
        lessons: summary.lessons,
        audio: summary.audio,
        questions: summary.questions,
    })
}

/// Translate `word` to Russian via MyMemory. Reuses a cached translation on
/// the word's dictionary entry if it has one; otherwise calls MyMemory and,
/// if the word is on the list, caches the result for next time.
///
/// The source language is the language of `lesson_id`'s book, falling back
/// to `TRANSLATE_SOURCE_LANG` when there's no lesson or the book has none.
#[tauri::command]
pub fn translate_word(
    state: State<DbState>,
    config: State<TranslateConfig>,
    word: String,
    lesson_id: Option<String>,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    if let Some(cached) = wordglow_core::dictionary::get_by_word(&conn, &word)
        .map_err(|e| e.to_string())?
        .and_then(|entry| entry.translation)
    {
        return Ok(cached);
    }

    let source_lang = match lesson_id.as_deref() {
        Some(id) => wordglow_core::translate::source_lang_for_lesson(&conn, id).map_err(|e| e.to_string())?,
        None => None,
    }
    .unwrap_or_else(|| config.translate_source_lang.clone());

    let translation =
        wordglow_core::translate::translate_word(&source_lang, config.mymemory_email.as_deref(), &word)
            .map_err(|e| e.to_string())?;

    wordglow_core::dictionary::set_translation(&conn, &word, &translation).map_err(|e| e.to_string())?;

    Ok(translation)
}
