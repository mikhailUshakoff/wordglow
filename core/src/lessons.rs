use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::hash::hash_text;
use crate::models::Lesson;
use crate::now_iso8601;

fn row_to_lesson(row: &Row) -> rusqlite::Result<Lesson> {
    Ok(Lesson {
        id: row.get(0)?,
        book_id: row.get(1)?,
        order_index: row.get(2)?,
        title: row.get(3)?,
        text: row.get(4)?,
        text_hash: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const SELECT_COLUMNS: &str =
    "id, book_id, order_index, title, text, text_hash, created_at, updated_at";

/// Canonicalizes `-`/`—` spacing so a dash always reads as a lead-in to the
/// word after it: exactly one space before, none after (e.g. "else—was",
/// "else-was", "else — was" and "else - was" all become "else —was" /
/// "else -was"). Without this, TTS and the reader's word-highlighting can
/// tokenize the same dash differently depending on how it was originally
/// spaced, drifting the highlight out of sync with the audio.
fn normalize_dashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '-' || c == '—' {
            while matches!(out.chars().last(), Some(c) if c.is_whitespace()) {
                out.pop();
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

pub fn create(
    conn: &Connection,
    book_id: &str,
    title: &str,
    text: &str,
    order_index: i64,
) -> Result<Lesson> {
    let text = &normalize_dashes(text);
    let id = Uuid::new_v4().to_string();
    let now = now_iso8601();
    let text_hash = hash_text(text);
    conn.execute(
        "INSERT INTO lessons (id, book_id, order_index, title, text, text_hash, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![id, book_id, order_index, title, text, text_hash, now],
    )?;
    Ok(Lesson {
        id,
        book_id: book_id.to_string(),
        order_index,
        title: title.to_string(),
        text: text.to_string(),
        text_hash,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Like [`create`], but appends after the book's current last lesson
/// instead of taking an explicit `order_index`.
pub fn create_appending(conn: &Connection, book_id: &str, title: &str, text: &str) -> Result<Lesson> {
    let next_order = list_for_book(conn, book_id)?
        .iter()
        .map(|l| l.order_index)
        .max()
        .map_or(0, |max| max + 1);
    create(conn, book_id, title, text, next_order)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Lesson>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM lessons WHERE id = ?1"),
        params![id],
        row_to_lesson,
    )
    .optional()
    .map_err(Into::into)
}

pub fn list_for_book(conn: &Connection, book_id: &str) -> Result<Vec<Lesson>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM lessons WHERE book_id = ?1 ORDER BY order_index"
    ))?;
    let rows = stmt.query_map(params![book_id], row_to_lesson)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Find lessons in `book_id` matching every given (non-`None`) criterion.
pub fn find(
    conn: &Connection,
    book_id: &str,
    id: Option<&str>,
    title: Option<&str>,
    text: Option<&str>,
) -> Result<Vec<Lesson>> {
    Ok(list_for_book(conn, book_id)?
        .into_iter()
        .filter(|l| {
            id.is_none_or(|v| l.id == v)
                && title.is_none_or(|v| l.title == v)
                && text.is_none_or(|v| l.text == v)
        })
        .collect())
}

/// Like [`find`], but errors unless exactly one lesson matches.
pub fn resolve_one(
    conn: &Connection,
    book_id: &str,
    id: Option<&str>,
    title: Option<&str>,
    text: Option<&str>,
) -> Result<Lesson> {
    let matches = find(conn, book_id, id, title, text)?;
    match matches.len() {
        0 => Err(CoreError::NotFound("lesson matching given criteria".into())),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(CoreError::Ambiguous(format!(
            "{} lessons match: {}",
            matches.len(),
            matches
                .iter()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

pub fn delete(conn: &Connection, lesson_id: &str) -> Result<()> {
    let changed = conn.execute("DELETE FROM lessons WHERE id = ?1", params![lesson_id])?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("lesson '{lesson_id}'")));
    }
    Ok(())
}

/// Reassign `order_index` for every lesson in `book_id` to match the
/// position of its id in `ordered_ids`. `ordered_ids` must contain
/// exactly the book's current lesson ids, in the desired order.
pub fn reorder(conn: &Connection, book_id: &str, ordered_ids: &[String]) -> Result<()> {
    let existing = list_for_book(conn, book_id)?;

    let mut existing_ids: Vec<&str> = existing.iter().map(|l| l.id.as_str()).collect();
    existing_ids.sort_unstable();
    let mut given_ids: Vec<&str> = ordered_ids.iter().map(String::as_str).collect();
    given_ids.sort_unstable();

    if existing_ids != given_ids {
        return Err(CoreError::InvalidReorder(
            "given lesson ids must match the book's current lessons exactly".into(),
        ));
    }

    let now = now_iso8601();
    let tx = conn.unchecked_transaction()?;
    for (index, lesson_id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE lessons SET order_index = ?1, updated_at = ?2 WHERE id = ?3",
            params![index as i64, now, lesson_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books;
    use crate::db::test_conn;

    fn setup_book(conn: &Connection) -> String {
        books::create(conn, "Dune", None, None).unwrap().id
    }

    #[test]
    fn create_computes_text_hash_and_lists_in_order() {
        let conn = test_conn();
        let book_id = setup_book(&conn);

        let l1 = create(&conn, &book_id, "Ch1", "hello world", 0).unwrap();
        let l2 = create(&conn, &book_id, "Ch2", "second", 1).unwrap();

        assert_eq!(l1.text_hash, crate::hash::hash_text("hello world"));

        let listed = list_for_book(&conn, &book_id).unwrap();
        assert_eq!(listed.iter().map(|l| &l.id).collect::<Vec<_>>(), vec![&l1.id, &l2.id]);
    }

    #[test]
    fn normalize_dashes_makes_every_spacing_variant_agree() {
        for text in ["else—was", "else-was", "else — was", "else - was"] {
            assert_eq!(normalize_dashes(text), if text.contains('—') { "else —was" } else { "else -was" });
        }
    }

    #[test]
    fn create_normalizes_dash_spacing_before_hashing() {
        let conn = test_conn();
        let book_id = setup_book(&conn);

        let lesson = create(&conn, &book_id, "Ch1", "else—was gone", 0).unwrap();

        assert_eq!(lesson.text, "else —was gone");
        assert_eq!(lesson.text_hash, crate::hash::hash_text("else —was gone"));
    }

    #[test]
    fn delete_removes_lesson() {
        let conn = test_conn();
        let book_id = setup_book(&conn);
        let lesson = create(&conn, &book_id, "Ch1", "text", 0).unwrap();

        delete(&conn, &lesson.id).unwrap();

        assert!(get(&conn, &lesson.id).unwrap().is_none());
        assert!(matches!(delete(&conn, &lesson.id), Err(CoreError::NotFound(_))));
    }

    #[test]
    fn resolve_one_by_title_and_ambiguous_when_duplicated() {
        let conn = test_conn();
        let book_id = setup_book(&conn);
        let a = create(&conn, &book_id, "Same", "a", 0).unwrap();

        assert_eq!(
            resolve_one(&conn, &book_id, None, Some("Same"), None)
                .unwrap()
                .id,
            a.id
        );

        create(&conn, &book_id, "Same", "b", 1).unwrap();
        assert!(matches!(
            resolve_one(&conn, &book_id, None, Some("Same"), None),
            Err(CoreError::Ambiguous(_))
        ));
    }

    #[test]
    fn reorder_updates_order_index_and_rejects_mismatched_sets() {
        let conn = test_conn();
        let book_id = setup_book(&conn);
        let l1 = create(&conn, &book_id, "Ch1", "a", 0).unwrap();
        let l2 = create(&conn, &book_id, "Ch2", "b", 1).unwrap();

        reorder(&conn, &book_id, &[l2.id.clone(), l1.id.clone()]).unwrap();

        let listed = list_for_book(&conn, &book_id).unwrap();
        assert_eq!(listed[0].id, l2.id);
        assert_eq!(listed[1].id, l1.id);

        assert!(matches!(
            reorder(&conn, &book_id, &[l1.id.clone()]),
            Err(CoreError::InvalidReorder(_))
        ));
    }
}
