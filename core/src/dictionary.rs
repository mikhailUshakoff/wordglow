use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::Result;
use crate::models::DictionaryEntry;
use crate::now_iso8601;

fn row_to_entry(row: &Row) -> rusqlite::Result<DictionaryEntry> {
    Ok(DictionaryEntry {
        id: row.get(0)?,
        word: row.get(1)?,
        translation: row.get(2)?,
        lesson_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

const SELECT_COLUMNS: &str = "id, word, translation, lesson_id, created_at";

pub fn get_by_word(conn: &Connection, word: &str) -> Result<Option<DictionaryEntry>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM dictionary WHERE word = ?1 COLLATE NOCASE"),
        params![word],
        row_to_entry,
    )
    .optional()
    .map_err(Into::into)
}

/// Save a word to the learner's word list. Words are deduplicated
/// case-insensitively — saving an already-saved word just returns the
/// existing entry.
pub fn add_word(conn: &Connection, word: &str, lesson_id: Option<&str>) -> Result<DictionaryEntry> {
    if let Some(existing) = get_by_word(conn, word)? {
        return Ok(existing);
    }

    let now = now_iso8601();
    conn.execute(
        "INSERT INTO dictionary (word, translation, lesson_id, created_at) VALUES (?1, NULL, ?2, ?3)",
        params![word, lesson_id, now],
    )?;
    Ok(DictionaryEntry {
        id: conn.last_insert_rowid(),
        word: word.to_string(),
        translation: None,
        lesson_id: lesson_id.map(str::to_string),
        created_at: now,
    })
}

pub fn remove_word(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM dictionary WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_all(conn: &Connection) -> Result<Vec<DictionaryEntry>> {
    let mut stmt = conn.prepare(&format!("SELECT {SELECT_COLUMNS} FROM dictionary ORDER BY created_at DESC"))?;
    let rows = stmt.query_map([], row_to_entry)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Cache a translation for a word already on the list (no-op if the word
/// was never saved).
pub fn set_translation(conn: &Connection, word: &str, translation: &str) -> Result<()> {
    conn.execute(
        "UPDATE dictionary SET translation = ?1 WHERE word = ?2 COLLATE NOCASE",
        params![translation, word],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_conn;

    #[test]
    fn add_word_deduplicates_case_insensitively() {
        let conn = test_conn();
        let first = add_word(&conn, "Hola", None).unwrap();
        let second = add_word(&conn, "hola", None).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(list_all(&conn).unwrap().len(), 1);
    }

    #[test]
    fn remove_word_deletes_entry() {
        let conn = test_conn();
        let entry = add_word(&conn, "adios", None).unwrap();
        remove_word(&conn, entry.id).unwrap();
        assert!(list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn set_translation_caches_value() {
        let conn = test_conn();
        add_word(&conn, "gracias", None).unwrap();
        set_translation(&conn, "gracias", "спасибо").unwrap();
        let entry = get_by_word(&conn, "gracias").unwrap().unwrap();
        assert_eq!(entry.translation.as_deref(), Some("спасибо"));
    }
}
