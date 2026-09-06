//! Content packages: zip files bundling books/lessons (+ cached audio and
//! questions) for the teacher to hand off to the student. Only content
//! columns are ever written on import — `lessons.status` is progress data
//! living on a content row and must survive re-importing the lesson it
//! belongs to.

use std::collections::HashSet;
use std::io::{Read, Seek, Write};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::{CoreError, Result};
use crate::models::{Book, Lesson, LessonAudio, Question};

const MANIFEST_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    books: Vec<Book>,
    lessons: Vec<Lesson>,
    lesson_audio: Vec<LessonAudio>,
    questions: Vec<Question>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImportSummary {
    pub books: usize,
    pub lessons: usize,
    pub audio: usize,
    pub questions: usize,
}

fn write_package<W: Write + Seek>(conn: &Connection, books: Vec<Book>, lessons: Vec<Lesson>, writer: W) -> Result<()> {
    let mut lesson_audio = Vec::new();
    let mut questions = Vec::new();
    for lesson in &lessons {
        if let Some(audio) = crate::tts::get_audio(conn, &lesson.id)? {
            lesson_audio.push(audio);
        }
        questions.extend(crate::questions::list_for_lesson(conn, &lesson.id)?);
    }

    let manifest = Manifest { version: MANIFEST_VERSION, books, lessons, lesson_audio: lesson_audio.clone(), questions };

    let mut zip = ZipWriter::new(writer);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("manifest.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    let audio_dir = crate::paths::audio_dir()?;
    for audio in &lesson_audio {
        if let Ok(bytes) = std::fs::read(audio_dir.join(&audio.audio_path)) {
            zip.start_file(format!("audio/{}", audio.audio_path), options)?;
            zip.write_all(&bytes)?;
        }
    }

    zip.finish()?;
    Ok(())
}

/// Export every book and lesson in the library.
pub fn export_library<W: Write + Seek>(conn: &Connection, writer: W) -> Result<()> {
    let books = crate::books::list_all(conn)?;
    let mut lessons = Vec::new();
    for book in &books {
        lessons.extend(crate::lessons::list_for_book(conn, &book.id)?);
    }
    write_package(conn, books, lessons, writer)
}

/// Export a single book and all of its lessons.
pub fn export_book<W: Write + Seek>(conn: &Connection, book_id: &str, writer: W) -> Result<()> {
    let book = crate::books::get(conn, book_id)?.ok_or_else(|| CoreError::NotFound(format!("book '{book_id}'")))?;
    let lessons = crate::lessons::list_for_book(conn, book_id)?;
    write_package(conn, vec![book], lessons, writer)
}

/// Export a single lesson, along with its parent book (needed so the
/// lesson's `book_id` foreign key resolves on import into a fresh library).
pub fn export_lesson<W: Write + Seek>(conn: &Connection, lesson_id: &str, writer: W) -> Result<()> {
    let lesson = crate::lessons::get(conn, lesson_id)?.ok_or_else(|| CoreError::NotFound(format!("lesson '{lesson_id}'")))?;
    let book = crate::books::get(conn, &lesson.book_id)?
        .ok_or_else(|| CoreError::NotFound(format!("book '{}'", lesson.book_id)))?;
    write_package(conn, vec![book], vec![lesson], writer)
}

/// Merge an imported package into `conn`. Books and lessons are upserted by
/// UUID; lesson audio and questions are replaced wholesale for any lesson
/// included in the package. `lessons.status` is never written by an insert
/// (defaults to `not_started`) nor touched by the update branch, so a
/// lesson's reading progress survives re-importing it.
pub fn import_package<R: Read + Seek>(conn: &Connection, reader: R) -> Result<ImportSummary> {
    let mut zip = ZipArchive::new(reader)?;

    let manifest: Manifest = {
        let mut file = zip.by_name("manifest.json")?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        serde_json::from_str(&buf)?
    };

    let audio_dir = crate::paths::audio_dir()?;
    let tx = conn.unchecked_transaction()?;

    for book in &manifest.books {
        tx.execute(
            "INSERT INTO books (id, title, author, language, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(id) DO UPDATE SET \
                title = excluded.title, author = excluded.author, \
                language = excluded.language, updated_at = excluded.updated_at",
            params![book.id, book.title, book.author, book.language, book.created_at, book.updated_at],
        )?;
    }

    for lesson in &manifest.lessons {
        tx.execute(
            "INSERT INTO lessons (id, book_id, order_index, title, text, text_hash, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'not_started', ?7, ?8) \
             ON CONFLICT(id) DO UPDATE SET \
                book_id = excluded.book_id, order_index = excluded.order_index, \
                title = excluded.title, text = excluded.text, text_hash = excluded.text_hash, \
                updated_at = excluded.updated_at",
            params![
                lesson.id,
                lesson.book_id,
                lesson.order_index,
                lesson.title,
                lesson.text,
                lesson.text_hash,
                lesson.created_at,
                lesson.updated_at
            ],
        )?;
    }

    for audio in &manifest.lesson_audio {
        tx.execute(
            "INSERT INTO lesson_audio (lesson_id, audio_path, voice, word_timepoints, generated_at, text_hash_at_gen) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(lesson_id) DO UPDATE SET \
                audio_path = excluded.audio_path, voice = excluded.voice, \
                word_timepoints = excluded.word_timepoints, generated_at = excluded.generated_at, \
                text_hash_at_gen = excluded.text_hash_at_gen",
            params![
                audio.lesson_id,
                audio.audio_path,
                audio.voice,
                serde_json::to_string(&audio.word_timepoints)?,
                audio.generated_at,
                audio.text_hash_at_gen
            ],
        )?;

        if let Ok(mut file) = zip.by_name(&format!("audio/{}", audio.audio_path)) {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            std::fs::write(audio_dir.join(&audio.audio_path), bytes)?;
        }
    }

    let touched_lessons: HashSet<&str> = manifest.lessons.iter().map(|l| l.id.as_str()).collect();
    for lesson_id in &touched_lessons {
        tx.execute("DELETE FROM questions WHERE lesson_id = ?1", params![lesson_id])?;
    }
    for question in &manifest.questions {
        tx.execute(
            "INSERT INTO questions (id, lesson_id, order_index, question_text, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![question.id, question.lesson_id, question.order_index, question.question_text, question.created_at],
        )?;
    }

    tx.commit()?;

    Ok(ImportSummary {
        books: manifest.books.len(),
        lessons: manifest.lessons.len(),
        audio: manifest.lesson_audio.len(),
        questions: manifest.questions.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_conn;
    use std::io::Cursor;

    #[test]
    fn export_then_import_round_trips_book_and_lessons() {
        let conn = test_conn();
        let book = crate::books::create(&conn, "Dune", Some("Frank Herbert"), Some("en")).unwrap();
        let lesson = crate::lessons::create(&conn, &book.id, "Ch1", "hello world", 0).unwrap();

        let mut buf = Cursor::new(Vec::new());
        export_book(&conn, &book.id, &mut buf).unwrap();

        let other = test_conn();
        buf.set_position(0);
        let summary = import_package(&other, buf).unwrap();

        assert_eq!(summary.books, 1);
        assert_eq!(summary.lessons, 1);
        assert_eq!(crate::books::get(&other, &book.id).unwrap().unwrap().title, "Dune");
        assert_eq!(crate::lessons::get(&other, &lesson.id).unwrap().unwrap().text, "hello world");
    }

    #[test]
    fn import_never_overwrites_existing_lesson_status() {
        let conn = test_conn();
        let book = crate::books::create(&conn, "Dune", None, None).unwrap();
        let lesson = crate::lessons::create(&conn, &book.id, "Ch1", "hello world", 0).unwrap();
        crate::lessons::set_status(&conn, &lesson.id, crate::models::LessonStatus::Completed).unwrap();

        let mut buf = Cursor::new(Vec::new());
        export_book(&conn, &book.id, &mut buf).unwrap();
        buf.set_position(0);
        import_package(&conn, buf).unwrap();

        assert_eq!(crate::lessons::get(&conn, &lesson.id).unwrap().unwrap().status, crate::models::LessonStatus::Completed);
    }

    #[test]
    fn export_lesson_includes_parent_book() {
        let conn = test_conn();
        let book = crate::books::create(&conn, "Dune", None, None).unwrap();
        let lesson = crate::lessons::create(&conn, &book.id, "Ch1", "hello world", 0).unwrap();

        let mut buf = Cursor::new(Vec::new());
        export_lesson(&conn, &lesson.id, &mut buf).unwrap();

        let other = test_conn();
        buf.set_position(0);
        let summary = import_package(&other, buf).unwrap();

        assert_eq!(summary.books, 1);
        assert_eq!(summary.lessons, 1);
        assert!(crate::books::get(&other, &book.id).unwrap().is_some());
    }
}
