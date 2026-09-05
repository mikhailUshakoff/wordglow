/// Each entry is applied once, in order, and bumps `PRAGMA user_version`
/// by one. Add new migrations by appending to this slice — never edit an
/// already-released one.
pub const MIGRATIONS: &[&str] = &[MIGRATION_0001_INIT];

const MIGRATION_0001_INIT: &str = r#"
CREATE TABLE books (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    author      TEXT,
    language    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE lessons (
    id           TEXT PRIMARY KEY,
    book_id      TEXT NOT NULL REFERENCES books(id),
    order_index  INTEGER NOT NULL,
    title        TEXT NOT NULL,
    text         TEXT NOT NULL,
    text_hash    TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE lesson_audio (
    lesson_id         TEXT PRIMARY KEY REFERENCES lessons(id),
    audio_path        TEXT NOT NULL,
    voice             TEXT NOT NULL,
    word_timepoints   TEXT NOT NULL,
    generated_at      TEXT NOT NULL,
    text_hash_at_gen  TEXT NOT NULL
);

CREATE TABLE questions (
    id             TEXT PRIMARY KEY,
    lesson_id      TEXT NOT NULL REFERENCES lessons(id),
    order_index    INTEGER NOT NULL,
    question_text  TEXT NOT NULL,
    created_at     TEXT NOT NULL
);
"#;
