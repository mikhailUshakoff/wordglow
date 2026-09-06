use std::sync::Mutex;

use rusqlite::Connection;
use tauri::State;

use crate::dto::{BookDto, LessonDto};

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
