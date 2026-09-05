# Wordglow — Specification

*Name note: "Wordglow" refers to the word-highlighting-while-listening mechanic that's central to the reading experience.*

One person plays both roles: **Teacher** (authors content) and **Student** (reads and learns), possibly on different machines. Fully local, no server. Rust-only stack.

## 1. Goals

- Teacher authors books split into lessons, generates read-aloud audio and comprehension questions for each.
- Student reads/listens to lessons with word-level highlighting, builds a personal dictionary, takes quizzes, and tracks progress through streaks, XP, and badges.
- Content is portable between machines (a package file, or bundled into a build); personal progress is not expected to travel and is fine staying wherever it was created.

## 2. Non-goals (v1)

- No multi-student accounts or leaderboards — this is a single learner's tool.
- No cloud sync of progress between machines.
- No editing lessons from the student side — content is read-only there; it only arrives via seed/import.

## 3. Architecture

### 3.1 Cargo workspace

```
wordglow/
  Cargo.toml           # [workspace] members = ["core", "teacher-cli", "wordglow"]
  core/                # lib: db schema/migrations, models, TTS + Ollama calls,
                        #      package export/import, merge logic, path resolution
  teacher-cli/         # bin: clap-based authoring commands, depends on core
  wordglow/            # bin: the student-facing Tauri + Leptos app, depends on core
```

`core` is the only place that touches the database, calls Google TTS/Ollama, or reads/writes package files. `teacher-cli` and `wordglow` are thin wrappers: one exposes `core` as CLI subcommands, the other exposes it as `#[tauri::command]`s.

Both binaries resolve the data directory the same way, via a helper in `core` built on the `directories` crate (`ProjectDirs`) — not Tauri's own `app_data_dir()`, since that API isn't available outside a Tauri context and `teacher-cli` needs it too.

### 3.2 Single database

One file, `wordglow.db`, holds everything. `teacher-cli` writes the **content tables** (`books`, `lessons`, `lesson_audio`, `questions`) locally; `wordglow` merges into those same tables on import/first-run, and separately writes its own **progress tables** (`dictionary`, `activity_log`, etc.) that content merges never touch.

The safety property a two-file split would give doesn't actually require two files — it just requires the merge logic to only ever touch the content tables. As long as `merge_content()` upserts by UUID into `books`/`lessons`/`lesson_audio`/`questions` and never writes to a progress table, importing new content can't clobber existing dictionary entries, streaks, or XP, even though it's all one file.

### 3.3 Content package format

One `.zip` format is used for every content transfer — a brand-new install and a single new lesson are the same operation at different sizes.

```
package.zip
  manifest.json               # package format version
  books/
    <book_uuid>/
      book.json                # title, author, language
      lessons/
        <lesson_uuid>/
          lesson.json          # title, order_index, text
          audio.mp3            # optional — present if TTS was already generated
          timing.json          # optional — word-level timepoints, present alongside audio.mp3
          questions.json       # optional — present if Ollama questions were already generated
```

- `teacher-cli export --output <file>` with no filter exports the whole library (used for the first send, bundled into the Tauri build's resources).
- `teacher-cli export --book <uuid>` or `--lesson <uuid>` exports just that subset (used for a single new lesson or book sent later).
- `wordglow` has one `merge_content(package_path)` function in `core`, called both on first run (against the bundled package) and from an "Import lesson" action (against a file the student picks). It **upserts by UUID into the content tables only**: an existing book/lesson is updated in place, a new one is inserted, and the progress tables are never written to during a merge.

### 3.4 Suggested crates

| Purpose | Crate |
|---|---|
| SQLite access | `rusqlite` (feature `bundled`, so SQLite compiles from vendored source — no system dependency) |
| UUIDs | `uuid` (v4 for generation, serde feature for JSON) |
| Serialization | `serde`, `serde_json` |
| Package files | `zip` |
| HTTP calls (Google TTS, Ollama) | `reqwest` |
| CLI parsing | `clap` |
| Data directory resolution | `directories` |
| Timestamps | `time` or `chrono` |

## 4. Data model

All tables live in one file, `wordglow.db`. Content tables use UUID primary keys (needed for merge-by-UUID on import); progress tables use plain autoincrement IDs, since they're never merged or exported and don't need globally-unique keys. Because everything is in one file, progress tables can hold real foreign keys back to content tables.

### 4.1 Content tables

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
    question_text  TEXT NOT NULL,
    choices        TEXT NOT NULL,     -- JSON array of strings
    correct_index  INTEGER NOT NULL,
    created_at     TEXT NOT NULL
);
```

### 4.2 Progress tables

```sql
CREATE TABLE dictionary (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    word              TEXT NOT NULL,
    translation       TEXT,
    source_lesson_id  TEXT REFERENCES lessons(id),
    context_sentence  TEXT,
    added_at          TEXT NOT NULL,
    next_review_at    TEXT,           -- spaced repetition state
    interval_days     REAL DEFAULT 1,
    ease_factor       REAL DEFAULT 2.5,
    review_count      INTEGER DEFAULT 0
);

CREATE TABLE activity_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_date TEXT NOT NULL,   -- YYYY-MM-DD, drives the calendar heatmap and streaks
    activity_type TEXT NOT NULL,   -- 'lesson_read' | 'quiz' | 'word_added' | 'word_reviewed'
    lesson_id     TEXT REFERENCES lessons(id),
    xp_earned     INTEGER DEFAULT 0,
    created_at    TEXT NOT NULL
);

CREATE TABLE quiz_attempts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    lesson_id    TEXT NOT NULL REFERENCES lessons(id),
    question_id  TEXT NOT NULL REFERENCES questions(id),
    correct      INTEGER NOT NULL,  -- 0/1
    answered_at  TEXT NOT NULL
);

CREATE TABLE lesson_progress (
    lesson_id         TEXT PRIMARY KEY REFERENCES lessons(id),
    last_position_ms  INTEGER DEFAULT 0,
    completed         INTEGER DEFAULT 0,
    completed_at      TEXT
);

CREATE TABLE badges_earned (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    badge_key  TEXT NOT NULL UNIQUE,
    earned_at  TEXT NOT NULL
);

CREATE TABLE user_stats (
    id                INTEGER PRIMARY KEY CHECK (id = 1),  -- single row
    total_xp          INTEGER DEFAULT 0,
    level             INTEGER DEFAULT 1,
    current_streak    INTEGER DEFAULT 0,
    longest_streak    INTEGER DEFAULT 0,
    last_active_date  TEXT
);
```

Since everything lives in one file, a query that needs both content and progress data (e.g. quiz questions alongside how the student answered them) is just a normal join — no `ATTACH DATABASE` needed.

## 5. Teacher features (`teacher-cli`)

| Command | Description |
|---|---|
| `add-book` | Create a book: title, author, language |
| `add-lesson` | Add a lesson to a book: title, text, order — repeatable to build up a book over time |
| `edit-lesson` / `delete-lesson` | Update or remove a lesson's text/title |
| `reorder-lessons` | Change a book's lesson order |
| `generate-tts` | Synthesize audio for a lesson via Google Cloud TTS with SSML `<mark>` tags; cache the audio file + word timepoints, keyed to the lesson's `text_hash` so stale edits are detectable |
| `generate-questions` | Call Ollama to produce 3–5 multiple-choice comprehension questions for a lesson |
| `export` | Package the whole library, or a filtered book/lesson, into a `.zip` for bundling into a build or sending directly |

## 6. Student features (`wordglow`)

**Library**
- Browse books and lessons; per-lesson status (not started / in progress / completed)
- Import a content package file sent by the teacher

**Reading & playback**
- Select a lesson to read
- Play / pause / stop, seek, adjustable playback speed
- Highlight the currently-spoken word, synced to the cached word timepoints
- Click a word anywhere in the text to jump playback there (resume-from-cursor)
- Tap/click a word to see its translation, or add it to the personal dictionary
- Auto-resume from the last read position when reopening a lesson

**Quiz**
- After finishing a lesson, take its generated quiz; score is recorded

**Dictionary**
- View and search saved words
- Remove words
- Spaced-repetition review mode: show a word, recall the translation, mark easy/hard — updates `interval_days`/`ease_factor`/`next_review_at` (simple SM-2-style scheduling)

**Progress & gamification**
- Calendar heatmap of activity, current/longest streak
- XP, levels, badges (see §9)
- Profile/home screen summarizing all of the above

## 7. Word highlighting & TTS

Recommended: **Google Cloud Text-to-Speech** (the real API, not the free `gTTS` wrapper) with SSML `<mark>` tags per word — the response includes exact timepoints alongside the audio, giving both a cacheable file and accurate sync in one call.

Fallback if avoiding a GCP account: `gTTS` (free) plus a forced-alignment pass (e.g. `aeneas`) run afterward to derive word timestamps from the generated audio.

## 8. Open question: translation lookup

Not yet decided. Clicking a word for its translation needs a data source — options include a self-hosted service like LibreTranslate, a bundled offline dictionary, or a paid translation API. Needs a decision before the dictionary/translation feature can be built.

## 9. Gamification detail

**XP**
- Finish a lesson: +10
- Correct quiz answer: +5 (bonus for a perfect lesson score)
- Add a word to the dictionary: +2
- Successfully recall a word in spaced-repetition review: +3
- Any activity on a given day: streak bonus

**Levels** (cumulative XP thresholds): Beginner Reader → Explorer → Adventurer → Scholar → Sage

**Badges**
- *Milestones:* first lesson finished, first book finished, first perfect quiz score
- *Consistency:* 3-day / 7-day / 30-day / 100-day streaks
- *Vocabulary:* 50 / 100 / 500 words added; 100 words successfully reviewed
- *Fun:* "Night owl" (reading after 10pm), "Comeback" (return after a break)

## 10. Open questions / pending decisions

- Translation data source (§8)
- Whether to actually set up Google Cloud billing, or start with the free fallback
- Exact SM-2 parameters for spaced repetition (can start with reasonable defaults and tune later)

## 11. Suggested build order

1. **Core CRUD** — `core` schema/migrations for `wordglow.db`; `teacher-cli` add-book/add-lesson; `wordglow` library browsing and plain text reading (no audio yet).
2. **TTS + highlighting** — `generate-tts`, audio playback, word-sync highlighting, seek/resume.
3. **Questions + quiz** — `generate-questions`, quiz UI and scoring.
4. **Dictionary** — word tap, translation (once §8 is resolved), dictionary view/remove, spaced-repetition review.
5. **Package export/import** — `teacher-cli export`, `merge_content`, first-run seeding, "Import lesson" in Wordglow.
6. **Gamification** — XP/levels/badges/streak calendar, profile screen.
