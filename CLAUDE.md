# Wordglow

Local-first reading & language-learning tutor. One person plays both **Teacher** (authors content) and **Student** (reads/learns), possibly on different machines. Rust-only stack, no server, no cloud sync.

## Tech stack

- **`wordglow`** (student-facing app): Tauri + Leptos, CSR mode (built via Trunk), running in the Tauri webview
- **`teacher-cli`**: plain Rust CLI using `clap`
- **`core`**: shared lib — schema/migrations, models, Google TTS + Ollama calls, package export/import, merge logic, data-directory path resolution
- SQLite via `rusqlite` (feature `bundled` — no system dependency)
- Other crates: `uuid` (v4, serde feature), `serde` / `serde_json`, `zip` (package export/import), `reqwest` (Google TTS + Ollama HTTP calls), `directories` (`ProjectDirs` for the data directory — used by *both* binaries so they agree on the same path)

## Workspace layout

```
wordglow/
  Cargo.toml           # [workspace] members = ["core", "teacher-cli", "wordglow"]
  core/                # lib
  teacher-cli/         # bin
  wordglow/            # bin — the Tauri + Leptos app
```

`core` is the only place that touches the databases or calls external services. Both binaries are thin wrappers: `teacher-cli` exposes `core` as CLI subcommands, `wordglow` exposes it as `#[tauri::command]`s.

## Database

One file: `wordglow.db`. No separate content/progress database split — just one SQLite file with content tables (`books`, `lessons`, `lesson_audio`, `questions`) and, later, progress tables (`dictionary`, `activity_log`, etc.) side by side.

The safety property that a split would give (importing content can't clobber progress data) doesn't need two files — it just needs `merge_content()` to only ever touch the content tables. Keep that rule even though it's all one file.

**For this phase**, only the content tables are needed. Don't create the progress tables (`dictionary`, `activity_log`, `quiz_attempts`, `lesson_progress`, `badges_earned`, `user_stats`) yet — they belong to later phases (dictionary/quiz/gamification), so leave them out of the migrations for now.

## Current implementation focus

Build only what's listed below. Don't implement dictionary, translation, quiz-taking/scoring, streaks, XP, or badges yet — those come in a later phase.

### Teacher CLI (`teacher-cli`)

| Command | Description |
|---|---|
| `add-book` | Create a book: title, author, language |
| `add-lesson` | Add a lesson to a book: title, text, order — repeatable to build up a book over time |
| `delete-lesson` | Remove a lesson (text/title) |
| `reorder-lessons` | Change a book's lesson order |
| `generate-tts` | Synthesize audio for a lesson via Google Cloud TTS with SSML `<mark>` tags; cache the audio file + word timepoints, keyed to the lesson's `text_hash` so stale edits are detectable |
| `generate-questions` | Call Ollama to produce 3–5 open (free-response) comprehension questions for a lesson |
| `export` | Package the whole library, or a filtered book/lesson, into a `.zip` for bundling into a build or sending directly |

### Wordglow app — Library

- Browse books and lessons; per-lesson status (not started / in progress / completed)
- Import a content package file sent by the teacher

### Wordglow app — Reading & playback

- Select a lesson to read
- Play / pause / stop, seek, adjustable playback speed
- Highlight the currently-spoken word, synced to the cached word timepoints
- Click a word anywhere in the text to jump playback there (resume-from-cursor)

Note: per-lesson status and resume-from-cursor only need in-memory/session state for now — the progress tables that would persist this aren't created yet. Persisting "last position" and "completed" across app restarts is a later-phase feature.

## Data model for this phase (`wordglow.db` — content tables only)

```sql
CREATE TABLE books (
    id          TEXT PRIMARY KEY,   -- UUID
    title       TEXT NOT NULL,
    author      TEXT,
    language    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE lessons (
    id           TEXT PRIMARY KEY,  -- UUID
    book_id      TEXT NOT NULL REFERENCES books(id),
    order_index  INTEGER NOT NULL,
    title        TEXT NOT NULL,
    text         TEXT NOT NULL,
    text_hash    TEXT NOT NULL,     -- hash of `text`; mismatch means cached audio/questions are stale
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE lesson_audio (
    lesson_id         TEXT PRIMARY KEY REFERENCES lessons(id),
    audio_path        TEXT NOT NULL,   -- relative path under the app's audio cache dir
    voice             TEXT NOT NULL,
    word_timepoints   TEXT NOT NULL,   -- JSON array of {word, start_ms, end_ms}
    generated_at      TEXT NOT NULL,
    text_hash_at_gen  TEXT NOT NULL
);

CREATE TABLE questions (
    id             TEXT PRIMARY KEY,  -- UUID
    lesson_id      TEXT NOT NULL REFERENCES lessons(id),
    order_index    INTEGER NOT NULL,
    question_text  TEXT NOT NULL,     -- open/free-response, no choices or correct_index
    created_at     TEXT NOT NULL
);
```

## Conventions

- Timestamps: ISO 8601 strings in `TEXT` columns
- IDs in content tables: UUID v4 as `TEXT` (not autoincrement — needed for merge-by-UUID on import). Progress tables, when they're added later, use plain autoincrement IDs.
- `text_hash`: hash of a lesson's `text` field (e.g. blake3 or sha256, stored as a hex string), used to detect when cached audio/questions are stale
- Errors: `thiserror` for typed errors inside `core`; `anyhow` at the CLI/Tauri-command boundary
- Audio cache path: `<data_dir>/audio/<lesson_id>.*`, resolved through `core`'s path helper — never hardcode a path in either binary

## External services

- **Google Cloud TTS**: real API (not the free `gTTS` wrapper). Send SSML with a `<mark>` per word; the response includes audio plus timepoints for each mark — store both.
- **Ollama**: remote HTTP API (default `http://localhost:11434`). Used only by `generate-questions`.

## Out of scope for now

- Dictionary, word translation, spaced repetition
- Quiz-taking / scoring (questions are generated and stored, but not yet surfaced as a gradable quiz)
- Streak calendar, XP, levels, badges
- `edit-lesson` (only `delete-lesson` exists right now)
- The progress tables (`dictionary`, `activity_log`, `quiz_attempts`, `lesson_progress`, `badges_earned`, `user_stats`) — not created yet, not part of this phase

## Suggested order for this phase

1. Workspace scaffold + `core` schema/migrations for `wordglow.db`
2. `teacher-cli`: `add-book`, `add-lesson`, `delete-lesson`, `reorder-lessons`
3. `generate-tts` (Google Cloud TTS + SSML marks, caching, `text_hash` staleness check)
4. `generate-questions` (Ollama call)
5. `wordglow` app: Library (browse)
6. `wordglow` app: Reading & playback (audio player + word highlighting + click-to-seek)
