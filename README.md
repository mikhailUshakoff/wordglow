# Wordglow

Local-first reading & language-learning tutor. One person plays both **Teacher** (authors content) and **Student** (reads/learns), possibly on different machines. Rust-only stack, no server, no cloud sync.

> **Status: proof of concept.** Code favors getting a working build out fast over efficiency — expect rough edges. The original goal during this phase was to cut down deployment time, not optimize the implementation.

## Features

- **Teacher CLI** — author books/lessons, generate TTS audio (Google Cloud TTS or free Edge TTS) with word-level timepoints, generate comprehension questions via Ollama, export packages
- **Wordglow app** (Tauri + Leptos) — browse your library, read along with synced audio + word highlighting, click any word to translate or save it to a vocabulary list

## Workspace layout

```
wordglow/
  core/          # shared lib: db schema/migrations, TTS + Ollama calls, package export/import
  teacher-cli/   # CLI for authoring content
  wordglow/      # Tauri + Leptos student-facing app
```

`core` is the only crate that touches the database or calls external services; both binaries are thin wrappers around it.

## Requirements

- Rust (stable), with the `wasm32-unknown-unknown` target for the app: `rustup target add wasm32-unknown-unknown`
- [`trunk`](https://trunkrs.dev/) and [`tauri-cli`](https://tauri.app/) for building the Wordglow app
- Optional: a Google Cloud TTS API key (for `generate-tts`) and a running Ollama instance (for `generate-questions` and in-app translation)

## Building

```bash
# workspace lib + CLI
cargo build

# Wordglow app (dev)
cd wordglow
trunk serve        # or: cargo tauri dev
```

## Teacher CLI

```bash
cargo run -p teacher-cli -- add-book --title "Treasure Island" --author "R. L. Stevenson" --language en-US
cargo run -p teacher-cli -- add-lesson --book "Treasure Island" --title "Ch1" --text-path ./ch1.txt
cargo run -p teacher-cli -- generate-tts --book "Treasure Island" --lesson-title "Ch1"
cargo run -p teacher-cli -- generate-questions --lesson-id <id>
cargo run -p teacher-cli -- export --book "Treasure Island" --output treasure-island.zip
```

Run `cargo run -p teacher-cli -- --help` for the full command list.

## Configuration

Environment variables (or a `.env` file), used by `teacher-cli`:

| Variable | Used by | Default |
|---|---|---|
| `GOOGLE_TTS_API_KEY` | `generate-tts` | required, no default |
| `OLLAMA_URL` | `generate-questions` | `https://ollama.com` |
| `OLLAMA_MODEL` | `generate-questions` | `gpt-oss:120b` |
| `OLLAMA_API_KEY` | `generate-questions` (Ollama's hosted cloud API only) | none — unneeded for a local Ollama server |

## Release builds

Pushing a `v*` tag triggers [`.github/workflows/release-windows.yml`](.github/workflows/release-windows.yml), which builds the Wordglow app and publishes a draft GitHub release for Windows.

## License

MIT — see [LICENSE](LICENSE).
