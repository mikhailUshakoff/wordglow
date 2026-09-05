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
