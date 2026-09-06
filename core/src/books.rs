use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::models::Book;
use crate::now_iso8601;

fn row_to_book(row: &Row) -> rusqlite::Result<Book> {
    Ok(Book {
        id: row.get(0)?,
        title: row.get(1)?,
        author: row.get(2)?,
        language: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

const SELECT_COLUMNS: &str = "id, title, author, language, created_at, updated_at";

pub fn create(
    conn: &Connection,
    title: &str,
    author: Option<&str>,
    language: Option<&str>,
) -> Result<Book> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso8601();
    conn.execute(
        "INSERT INTO books (id, title, author, language, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, title, author, language, now],
    )?;
    Ok(Book {
        id,
        title: title.to_string(),
        author: author.map(str::to_string),
        language: language.map(str::to_string),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_all(conn: &Connection) -> Result<Vec<Book>> {
    let mut stmt = conn.prepare(&format!("SELECT {SELECT_COLUMNS} FROM books ORDER BY title"))?;
    let rows = stmt.query_map([], row_to_book)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Delete a book along with every lesson in it, and each lesson's cached
/// audio (file on disk + row) and generated questions.
pub fn delete_cascade(conn: &Connection, book_id: &str) -> Result<()> {
    let lessons = crate::lessons::list_for_book(conn, book_id)?;

    let tx = conn.unchecked_transaction()?;
    for lesson in &lessons {
        let audio_path: Option<String> = tx
            .query_row(
                "SELECT audio_path FROM lesson_audio WHERE lesson_id = ?1",
                params![lesson.id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(audio_path) = audio_path {
            let _ = std::fs::remove_file(crate::paths::audio_dir()?.join(audio_path));
        }

        tx.execute("DELETE FROM lesson_audio WHERE lesson_id = ?1", params![lesson.id])?;
        tx.execute("DELETE FROM questions WHERE lesson_id = ?1", params![lesson.id])?;
        tx.execute("DELETE FROM lessons WHERE id = ?1", params![lesson.id])?;
    }

    let changed = tx.execute("DELETE FROM books WHERE id = ?1", params![book_id])?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("book '{book_id}'")));
    }

    tx.commit()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Book>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM books WHERE id = ?1"),
        params![id],
        row_to_book,
    )
    .optional()
    .map_err(Into::into)
}

pub fn find_by_title(conn: &Connection, title: &str) -> Result<Vec<Book>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM books WHERE title = ?1 COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map(params![title], row_to_book)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Resolve a book from a CLI-supplied reference that may be either its
/// UUID or its title.
pub fn resolve(conn: &Connection, reference: &str) -> Result<Book> {
    if let Some(book) = get(conn, reference)? {
        return Ok(book);
    }

    let matches = find_by_title(conn, reference)?;
    match matches.len() {
        0 => Err(CoreError::NotFound(format!("book '{reference}'"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(CoreError::Ambiguous(format!(
            "{} books titled '{reference}': {}",
            matches.len(),
            matches
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_conn;

    #[test]
    fn delete_cascade_removes_lessons_audio_and_questions() {
        let conn = test_conn();
        let book = create(&conn, "Dune", None, None).unwrap();
        let lesson = crate::lessons::create(&conn, &book.id, "Ch1", "hello world", 0).unwrap();
        conn.execute(
            "INSERT INTO lesson_audio (lesson_id, audio_path, voice, word_timepoints, generated_at, text_hash_at_gen) \
             VALUES (?1, ?2, 'en-US', '[]', 'now', ?3)",
            params![lesson.id, format!("{}.mp3", lesson.id), lesson.text_hash],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO questions (id, lesson_id, order_index, question_text, created_at) \
             VALUES ('q1', ?1, 0, 'What happened?', 'now')",
            params![lesson.id],
        )
        .unwrap();

        delete_cascade(&conn, &book.id).unwrap();

        assert!(get(&conn, &book.id).unwrap().is_none());
        assert!(crate::lessons::get(&conn, &lesson.id).unwrap().is_none());
        let audio_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM lesson_audio WHERE lesson_id = ?1", params![lesson.id], |r| r.get(0))
            .unwrap();
        assert_eq!(audio_count, 0);
        let question_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM questions WHERE lesson_id = ?1", params![lesson.id], |r| r.get(0))
            .unwrap();
        assert_eq!(question_count, 0);
    }

    #[test]
    fn delete_cascade_errors_on_unknown_book() {
        let conn = test_conn();
        assert!(matches!(delete_cascade(&conn, "nope"), Err(CoreError::NotFound(_))));
    }

    #[test]
    fn list_all_orders_by_title() {
        let conn = test_conn();
        create(&conn, "Zebra", None, None).unwrap();
        create(&conn, "Apple", None, None).unwrap();

        let titles: Vec<String> = list_all(&conn).unwrap().into_iter().map(|b| b.title).collect();
        assert_eq!(titles, vec!["Apple", "Zebra"]);
    }

    #[test]
    fn create_and_resolve_by_id_or_title() {
        let conn = test_conn();
        let book = create(&conn, "Dune", Some("Frank Herbert"), Some("en")).unwrap();

        assert_eq!(resolve(&conn, &book.id).unwrap().id, book.id);
        assert_eq!(resolve(&conn, "dune").unwrap().id, book.id); // case-insensitive
        assert!(matches!(
            resolve(&conn, "nope"),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn resolve_ambiguous_title() {
        let conn = test_conn();
        create(&conn, "Dune", None, None).unwrap();
        create(&conn, "Dune", None, None).unwrap();

        assert!(matches!(
            resolve(&conn, "Dune"),
            Err(CoreError::Ambiguous(_))
        ));
    }
}
