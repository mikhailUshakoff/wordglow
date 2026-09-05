use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;
use crate::migrations::MIGRATIONS;

/// Open the database at `path`, creating and migrating it if needed.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open the database at the app's standard data-directory location.
pub fn open_default() -> Result<Connection> {
    open(&crate::paths::db_path()?)
}

fn migrate(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let current = current as usize;

    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        conn.execute_batch(sql)?;
        let new_version = i + 1;
        conn.pragma_update(None, "user_version", new_version as i64)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    migrate(&conn).unwrap();
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_fresh_db_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        assert_eq!(
            tables,
            vec!["books", "lesson_audio", "lessons", "questions"]
        );
    }
}
