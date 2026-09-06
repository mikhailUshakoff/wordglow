use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

/// Teacher-side CLI for authoring Wordglow content.
#[derive(Parser)]
#[command(name = "teacher-cli")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List every book and its id
    ListBooks,

    /// Create a new book
    AddBook {
        #[arg(long)]
        title: String,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        language: Option<String>,
    },

    /// Add a lesson to a book (repeatable, to build up a book over time).
    /// Appended after the book's current last lesson.
    AddLesson {
        /// Book id or exact title
        #[arg(long)]
        book: String,
        #[arg(long)]
        title: String,
        /// Path to a .txt file containing the lesson text
        #[arg(long)]
        text_path: PathBuf,
    },

    /// Remove a book, and every lesson/audio/question belonging to it
    RemoveBook {
        /// Book id or exact title
        #[arg(long)]
        book: String,
    },

    /// Remove a lesson, identified by id, title, or text within a book
    DeleteLesson {
        /// Book id or exact title
        #[arg(long)]
        book: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        text: Option<String>,
    },

    /// Change a book's lesson order
    ReorderLessons {
        /// Book id or exact title
        #[arg(long)]
        book: String,
        /// Every lesson id in the book, comma-separated, in the desired order
        #[arg(long, value_delimiter = ',')]
        order: Vec<String>,
    },

    /// Synthesize lesson audio via Google Cloud TTS, with word timepoints.
    /// Skips regeneration if cached audio already matches the lesson's text.
    GenerateTts {
        /// Book id or exact title
        #[arg(long)]
        book: String,
        #[arg(long)]
        lesson_id: Option<String>,
        #[arg(long)]
        lesson_title: Option<String>,
        #[arg(long)]
        lesson_text: Option<String>,
        /// BCP-47 code, e.g. "en-US". Falls back to the book's language.
        #[arg(long)]
        language_code: Option<String>,
        /// e.g. "en-US-Wavenet-D". Left unset, Google picks a default voice.
        #[arg(long)]
        voice_name: Option<String>,
        #[arg(long)]
        ssml_gender: Option<String>,
        /// Regenerate even if cached audio already matches the lesson's text
        #[arg(long)]
        force: bool,
    },

    /// Synthesize lesson audio via the free, unofficial Microsoft Edge
    /// "read aloud" TTS (no API key needed) — MVP workaround for
    /// `generate-tts` when a Google Cloud TTS key isn't available.
    /// Skips regeneration if cached audio already matches the lesson's text.
    GenerateTtsFree {
        #[arg(long)]
        lesson_id: String,
        /// Edge voice short name, e.g. "en-US-AriaNeural"
        #[arg(long, default_value = "en-US-AvaNeural")]
        voice_name: String,
        /// Regenerate even if cached audio already matches the lesson's text
        #[arg(long)]
        force: bool,
    },

    /// Call Ollama to produce 3-5 open comprehension questions for a lesson,
    /// replacing any questions previously generated for it.
    GenerateQuestions {
        #[arg(long)]
        lesson_id: String,
    },
}

/// Ollama connection settings, loaded from the environment (or a `.env`
/// file) via `envy`. Both fields have defaults, so no env vars are required.
#[derive(serde::Deserialize)]
struct OllamaEnv {
    #[serde(default = "OllamaEnv::default_url")]
    ollama_url: String,
    #[serde(default = "OllamaEnv::default_model")]
    ollama_model: String,
    /// Bearer token for Ollama's hosted cloud API. Not needed for a local
    /// Ollama server.
    ollama_api_key: Option<String>,
}

impl OllamaEnv {
    fn default_url() -> String {
        "https://ollama.com".to_string()
    }

    fn default_model() -> String {
        "gpt-oss:120b".to_string()
    }
}

fn resolve_lesson(
    conn: &rusqlite::Connection,
    book_id: &str,
    id: Option<&str>,
    title: Option<&str>,
    text: Option<&str>,
) -> anyhow::Result<core::models::Lesson> {
    if id.is_none() && title.is_none() && text.is_none() {
        anyhow::bail!("at least one of --id/--lesson-id, --title/--lesson-title, --text/--lesson-text is required");
    }
    Ok(core::lessons::resolve_one(conn, book_id, id, title, text)?)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Opening the db also runs migrations, so the schema exists after
    // the very first invocation.
    let conn = core::db::open_default()?;

    match cli.command {
        None => println!("wordglow.db ready at {:?}", core::paths::db_path()?),

        Some(Command::ListBooks) => {
            let books = core::books::list_all(&conn)?;
            if books.is_empty() {
                println!("no books yet");
            }
            for book in books {
                println!("{}  {}", book.id, book.title);
            }
        }

        Some(Command::AddBook {
            title,
            author,
            language,
        }) => {
            let book = core::books::create(&conn, &title, author.as_deref(), language.as_deref())?;
            println!("created book {} ({})", book.id, book.title);
        }

        Some(Command::AddLesson {
            book,
            title,
            text_path,
        }) => {
            let text = std::fs::read_to_string(&text_path)
                .with_context(|| format!("reading lesson text from {}", text_path.display()))?;
            let book = core::books::resolve(&conn, &book)?;
            let lesson = core::lessons::create_appending(&conn, &book.id, &title, &text)?;
            println!(
                "created lesson {} in book '{}' at position {}",
                lesson.id, book.title, lesson.order_index
            );
        }

        Some(Command::RemoveBook { book }) => {
            let book = core::books::resolve(&conn, &book)?;
            core::books::delete_cascade(&conn, &book.id)?;
            println!("removed book {} ('{}') and all its lessons/audio/questions", book.id, book.title);
        }

        Some(Command::DeleteLesson {
            book,
            id,
            title,
            text,
        }) => {
            let book = core::books::resolve(&conn, &book)?;
            let lesson = resolve_lesson(&conn, &book.id, id.as_deref(), title.as_deref(), text.as_deref())?;
            core::lessons::delete(&conn, &lesson.id)?;
            println!("deleted lesson {} ('{}')", lesson.id, lesson.title);
        }

        Some(Command::ReorderLessons { book, order }) => {
            let book = core::books::resolve(&conn, &book)?;
            let count = order.len();
            core::lessons::reorder(&conn, &book.id, &order)?;
            println!("reordered {count} lessons in book '{}'", book.title);
        }

        Some(Command::GenerateTts {
            book,
            lesson_id,
            lesson_title,
            lesson_text,
            language_code,
            voice_name,
            ssml_gender,
            force,
        }) => {
            let book = core::books::resolve(&conn, &book)?;
            let lesson = resolve_lesson(
                &conn,
                &book.id,
                lesson_id.as_deref(),
                lesson_title.as_deref(),
                lesson_text.as_deref(),
            )?;

            let language_code = language_code
                .or(book.language)
                .ok_or_else(|| anyhow::anyhow!("--language-code is required (book has no language set)"))?;

            dotenvy::dotenv().ok();
            let api_key = std::env::var("GOOGLE_TTS_API_KEY")
                .context("set GOOGLE_TTS_API_KEY (in the environment or a .env file)")?;

            let voice = core::tts::VoiceConfig {
                language_code,
                voice_name,
                ssml_gender,
            };

            match core::tts::generate_tts(&conn, &api_key, &lesson, voice, force)? {
                core::tts::GenerateOutcome::UpToDate(audio) => println!(
                    "audio already up to date for lesson '{}' (generated {}); use --force to regenerate",
                    lesson.title, audio.generated_at
                ),
                core::tts::GenerateOutcome::Generated(audio) => println!(
                    "generated audio for lesson '{}' -> {} ({} words, voice '{}')",
                    lesson.title,
                    audio.audio_path,
                    audio.word_timepoints.len(),
                    audio.voice
                ),
            }
        }

        Some(Command::GenerateTtsFree {
            lesson_id,
            voice_name,
            force,
        }) => {
            let lesson = core::lessons::get(&conn, &lesson_id)?
                .ok_or_else(|| anyhow::anyhow!("no lesson with id {lesson_id}"))?;

            match core::tts::generate_tts_free(&conn, &lesson, &voice_name, force)? {
                core::tts::GenerateOutcome::UpToDate(audio) => println!(
                    "audio already up to date for lesson '{}' (generated {}); use --force to regenerate",
                    lesson.title, audio.generated_at
                ),
                core::tts::GenerateOutcome::Generated(audio) => println!(
                    "generated audio for lesson '{}' -> {} ({} words, voice '{}')",
                    lesson.title,
                    audio.audio_path,
                    audio.word_timepoints.len(),
                    audio.voice
                ),
            }
        }

        Some(Command::GenerateQuestions { lesson_id }) => {
            let lesson = core::lessons::get(&conn, &lesson_id)?
                .ok_or_else(|| anyhow::anyhow!("no lesson with id {lesson_id}"))?;

            dotenvy::dotenv().ok();
            let env: OllamaEnv = envy::from_env().context("reading OLLAMA_URL / OLLAMA_MODEL from the environment")?;

            let questions = core::questions::generate_questions(
                &conn,
                &env.ollama_url,
                env.ollama_api_key.as_deref(),
                &env.ollama_model,
                &lesson,
            )?;

            println!(
                "generated {} question(s) for lesson '{}' (model '{}')",
                questions.len(),
                lesson.title,
                env.ollama_model
            );
            for q in &questions {
                println!("  {}. {}", q.order_index + 1, q.question_text);
            }
        }
    }

    Ok(())
}
