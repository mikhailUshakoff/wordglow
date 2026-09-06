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

One file: `wordglow.db`. No separate content/progress database split — just one SQLite file with content tables (`books`, `lessons`, `lesson_audio`, `questions`), the `dictionary` (vocabulary/word-list) table, and, later, further progress tables (`activity_log`, etc.) side by side. Per-lesson reading status lives directly on `lessons.status` rather than a separate table.

The safety property that a split would give (importing content can't clobber progress data) doesn't need two files — it just needs `merge_content()` to only ever touch the content columns. Since `lessons.status` is progress data living on a content row, `merge_content()` must upsert lessons without touching the `status` column (explicit column list, never a blanket overwrite).

Pre-release, there's no installed base to migrate — `MIGRATIONS` is a single squashed migration. Once this ships, go back to append-only migrations (never edit a released one).

**For this phase**, the content tables plus `dictionary` are needed. Don't create the remaining progress tables (`activity_log`, `quiz_attempts`, `badges_earned`, `user_stats`) yet — they belong to later phases (quiz/gamification), so leave them out of the migrations for now.

## Current implementation focus

Build only what's listed below. Don't implement quiz-taking/scoring, streaks, XP, or badges yet — those come in a later phase. Spaced repetition on the word list is also a later phase — the vocabulary list itself (save/translate/remove) is in scope now.

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
- Click a word anywhere in the text pauses playback and opens a per-word action menu: continue playing from that word, translate it to Russian (via Ollama), or save it to the vocabulary list

Per-lesson status (not started / in progress / completed) is persisted in `lessons.status` and survives app restarts. Resume-from-cursor position is still in-memory/session-only for now — persisting "last position" across restarts is a later-phase feature.

### Wordglow app — Vocabulary

- A "Vocabulary" screen, reachable from the library, lists every saved word
- Per word: view its (cached) Russian translation, or remove it from the list

## Data model for this phase (`wordglow.db` — content tables + dictionary)

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
    status       TEXT NOT NULL DEFAULT 'not_started',  -- 'not_started' | 'in_progress' | 'completed'; progress data on a content row, see note above
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

CREATE TABLE dictionary (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,  -- progress table: plain autoincrement, not UUID
    word         TEXT NOT NULL,
    translation  TEXT,               -- cached Russian translation; NULL until first looked up
    lesson_id    TEXT REFERENCES lessons(id),
    created_at   TEXT NOT NULL
);
-- word is unique case-insensitively (UNIQUE INDEX ... COLLATE NOCASE) — saving
-- an already-saved word is a no-op that returns the existing entry.
```

## Conventions

- Timestamps: ISO 8601 strings in `TEXT` columns
- IDs in content tables: UUID v4 as `TEXT` (not autoincrement — needed for merge-by-UUID on import). Progress tables, when they're added later, use plain autoincrement IDs.
- `text_hash`: hash of a lesson's `text` field (e.g. blake3 or sha256, stored as a hex string), used to detect when cached audio/questions are stale
- Errors: `thiserror` for typed errors inside `core`; `anyhow` at the CLI/Tauri-command boundary
- Audio cache path: `<data_dir>/audio/<lesson_id>.*`, resolved through `core`'s path helper — never hardcode a path in either binary

## External services

- **Google Cloud TTS**: real API (not the free `gTTS` wrapper). Send SSML with a `<mark>` per word; the response includes audio plus timepoints for each mark — store both.
- **Ollama**: remote HTTP API (default `http://localhost:11434`). Used by `generate-questions`, and by the wordglow app's word-translation feature (translate to Russian).

## Out of scope for now

- Quiz-taking / scoring (questions are generated and stored, but not yet surfaced as a gradable quiz)
- Streak calendar, XP, levels, badges
- Spaced repetition / review scheduling for the vocabulary list
- `edit-lesson` (only `delete-lesson` exists right now)
- The remaining progress tables (`activity_log`, `quiz_attempts`, `badges_earned`, `user_stats`) — not created yet, not part of this phase
- Persisting resume-from-cursor position across restarts (lesson status is persisted; last-read position is not)

## Suggested order for this phase

1. Workspace scaffold + `core` schema/migrations for `wordglow.db`
2. `teacher-cli`: `add-book`, `add-lesson`, `delete-lesson`, `reorder-lessons`
3. `generate-tts` (Google Cloud TTS + SSML marks, caching, `text_hash` staleness check)
4. `generate-questions` (Ollama call)
5. `wordglow` app: Library (browse)
6. `wordglow` app: Reading & playback (audio player + word highlighting + click-to-seek)
7. `wordglow` app: Vocabulary (per-word action menu on click, translate via Ollama, saved-words list)
