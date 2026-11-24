# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.3] - 2026-11-24

### Changed

- In-app word translation (to Russian) now uses the free [MyMemory](https://mymemory.translated.net/doc/spec.php) API instead of Ollama, since Ollama's hosted API no longer allows free requests. No API key is needed.
- The Wordglow app no longer reads `OLLAMA_URL`, `OLLAMA_MODEL` or `OLLAMA_API_KEY`. `teacher-cli generate-questions` still uses Ollama.

### Added

- The source language for a translation is taken from the word's book (`add-book --language`, e.g. `en-US` → `en`). English is used when the book has no language set, or the saved word isn't linked to a lesson.
- Optional `MYMEMORY_EMAIL` environment variable: raises MyMemory's free quota from about 5k to about 50k characters per day.
- Translation errors from MyMemory, such as an exhausted daily quota, are shown as errors instead of being saved as the translation.
- The GitHub release description is taken from this changelog.

### Fixed

- The Windows installer was always named version 0.1.0 (e.g. `Wordglow_0.1.0_x64-setup.exe`). It now uses the workspace version from `Cargo.toml`, and the release workflow fails if the tag doesn't match that version.

## [0.0.2] - 2026-11-04

### Added

- The app version is shown in the Wordglow UI.
- `HOW_TO_ADD_BOOK.md` guide, plus OCR helper scripts in `scripts/` for turning scanned book spreads into lesson text.

### Fixed

- Word highlighting during playback no longer drifts out of sync on tokens with no letters or digits (e.g. `–`). They are no longer treated as spoken words.

## [0.0.1] - 2026-09-07

First release.

### Added

- **Teacher CLI**: `list-books`, `add-book`, `remove-book`, `list-lessons`, `add-lesson`, `change-lesson-text`, `delete-lesson`, `reorder-lessons`, `count-words`.
- `generate-tts`: lesson audio with word timepoints via Google Cloud TTS (SSML `<mark>`s). `generate-tts-free`: the same via free Microsoft Edge TTS. Audio is cached against the lesson's `text_hash`.
- `generate-questions`: 3–5 open comprehension questions per lesson via Ollama.
- `export`: package the library, a book or a lesson as a `.zip`.
- **Wordglow app**: library browser with per-lesson status (not started / in progress / completed) that survives restarts, and import of content packages.
- Reading view: play / pause / stop, seek, adjustable speed, the spoken word highlighted in sync with the audio, and the lesson's questions listed.
- Click any word to continue playback from it, translate it to Russian, or save it to the vocabulary list.
- Vocabulary screen: saved words with cached translations, and removing words.
- Windows release build via GitHub Actions on `v*` tags.

[0.0.3]: https://github.com/mikhailUshakoff/wordglow/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/mikhailUshakoff/wordglow/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/mikhailUshakoff/wordglow/releases/tag/v0.0.1
