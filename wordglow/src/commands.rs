use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
use tauri::State;

use crate::dto::{BookDto, LessonAudioDto, LessonDetailDto, LessonDto, WordTimepointDto};

pub struct DbState(pub Mutex<Connection>);

fn book_to_dto(book: wordglow_core::models::Book) -> BookDto {
    BookDto {
        id: book.id,
        title: book.title,
        author: book.author,
        language: book.language,
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
