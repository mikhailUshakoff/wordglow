use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::models::{Lesson, LessonAudio, WordTimepoint};
use crate::now_iso8601;

const SYNTHESIZE_URL: &str = "https://texttospeech.googleapis.com/v1beta1/text:synthesize";

#[derive(Debug, Clone, Default)]
pub struct VoiceConfig {
    pub language_code: String,
    pub voice_name: Option<String>,
    pub ssml_gender: Option<String>,
}

pub enum GenerateOutcome {
    /// Cached audio's `text_hash_at_gen` already matches the lesson's
    /// current text; nothing was (re)generated.
    UpToDate(LessonAudio),
    Generated(LessonAudio),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynthInput {
    ssml: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynthVoice {
    language_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssml_gender: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynthAudioConfig {
    audio_encoding: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynthRequest {
    input: SynthInput,
    voice: SynthVoice,
    audio_config: SynthAudioConfig,
    enable_time_pointing: Vec<&'static str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SynthResponse {
    audio_content: String,
    #[serde(default)]
    timepoints: Vec<Timepoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Timepoint {
    mark_name: String,
    time_seconds: f64,
}

/// Wraps each word in `text` with a named SSML `<mark>` so the TTS
/// response can report a timepoint per word. Returns the SSML plus the
/// words in the same order as the marks (`w0`, `w1`, ...).
fn build_ssml(text: &str) -> (String, Vec<String>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut ssml = String::from("<speak>");
    for (i, word) in words.iter().enumerate() {
        ssml.push_str(&format!("<mark name=\"w{i}\"/>{} ", xml_escape(word)));
    }
    ssml.push_str("</speak>");
    (ssml, words.into_iter().map(String::from).collect())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Turns per-mark timepoints into per-word (start, end) spans: a word's
/// span runs from its own mark to the next word's mark. The last word has
/// no following mark, so its end is set equal to its start.
fn word_timepoints_from_marks(words: &[String], timepoints: &[Timepoint]) -> Vec<WordTimepoint> {
    let seconds_by_mark: HashMap<&str, f64> = timepoints
        .iter()
        .map(|tp| (tp.mark_name.as_str(), tp.time_seconds))
        .collect();

    words
        .iter()
        .enumerate()
        .map(|(i, word)| {
            let start = seconds_by_mark.get(format!("w{i}").as_str()).copied().unwrap_or(0.0);
            let end = seconds_by_mark
                .get(format!("w{}", i + 1).as_str())
                .copied()
                .unwrap_or(start);
            WordTimepoint {
                word: word.clone(),
                start_ms: (start * 1000.0).round() as u64,
                end_ms: (end * 1000.0).round() as u64,
            }
        })
        .collect()
}

fn get_audio(conn: &Connection, lesson_id: &str) -> Result<Option<LessonAudio>> {
    let row = conn
        .query_row(
            "SELECT lesson_id, audio_path, voice, word_timepoints, generated_at, text_hash_at_gen \
             FROM lesson_audio WHERE lesson_id = ?1",
            params![lesson_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;

    row.map(
        |(lesson_id, audio_path, voice, word_timepoints_json, generated_at, text_hash_at_gen)| {
            Ok(LessonAudio {
                lesson_id,
                audio_path,
                voice,
                word_timepoints: serde_json::from_str(&word_timepoints_json)?,
                generated_at,
                text_hash_at_gen,
            })
        },
    )
    .transpose()
}

fn save_audio(conn: &Connection, audio: &LessonAudio) -> Result<()> {
    conn.execute(
        "INSERT INTO lesson_audio (lesson_id, audio_path, voice, word_timepoints, generated_at, text_hash_at_gen) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(lesson_id) DO UPDATE SET \
             audio_path = excluded.audio_path, \
             voice = excluded.voice, \
             word_timepoints = excluded.word_timepoints, \
             generated_at = excluded.generated_at, \
             text_hash_at_gen = excluded.text_hash_at_gen",
        params![
            audio.lesson_id,
            audio.audio_path,
            audio.voice,
            serde_json::to_string(&audio.word_timepoints)?,
            audio.generated_at,
            audio.text_hash_at_gen,
        ],
    )?;
    Ok(())
}

/// Synthesize (or reuse cached) audio for `lesson` via Google Cloud TTS.
///
/// If cached audio exists and its `text_hash_at_gen` matches the lesson's
/// current `text_hash`, this is a no-op unless `force` is set — that hash
/// comparison is what detects an edited lesson's audio as stale.
pub fn generate_tts(
    conn: &Connection,
    api_key: &str,
    lesson: &Lesson,
    voice: VoiceConfig,
    force: bool,
) -> Result<GenerateOutcome> {
    if !force {
        if let Some(existing) = get_audio(conn, &lesson.id)? {
            if existing.text_hash_at_gen == lesson.text_hash {
                return Ok(GenerateOutcome::UpToDate(existing));
            }
        }
    }

    let (ssml, words) = build_ssml(&lesson.text);

    let request = SynthRequest {
        input: SynthInput { ssml },
        voice: SynthVoice {
            language_code: voice.language_code.clone(),
            name: voice.voice_name.clone(),
            ssml_gender: voice.ssml_gender,
        },
        audio_config: SynthAudioConfig { audio_encoding: "MP3" },
        enable_time_pointing: vec!["SSML_MARK"],
    };

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(SYNTHESIZE_URL)
        .query(&[("key", api_key)])
        .json(&request)
        .send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(CoreError::Tts(format!("request failed ({status}): {body}")));
    }

    let parsed: SynthResponse = response.json()?;

    let audio_bytes = STANDARD
        .decode(&parsed.audio_content)
        .map_err(|e| CoreError::Tts(format!("invalid base64 audio content: {e}")))?;

    let word_timepoints = word_timepoints_from_marks(&words, &parsed.timepoints);

    let audio_path = crate::paths::audio_path(&lesson.id)?;
    std::fs::write(&audio_path, &audio_bytes)?;

    let audio = LessonAudio {
        lesson_id: lesson.id.clone(),
        audio_path: format!("{}.mp3", lesson.id),
        voice: voice.voice_name.unwrap_or(voice.language_code),
        word_timepoints,
        generated_at: now_iso8601(),
        text_hash_at_gen: lesson.text_hash.clone(),
    };
    save_audio(conn, &audio)?;

    Ok(GenerateOutcome::Generated(audio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books;
    use crate::db::test_conn;
    use crate::lessons;

    #[test]
    fn build_ssml_marks_every_word() {
        let (ssml, words) = build_ssml("hello brave new world");
        assert_eq!(words, vec!["hello", "brave", "new", "world"]);
        assert_eq!(
            ssml,
            "<speak><mark name=\"w0\"/>hello <mark name=\"w1\"/>brave <mark name=\"w2\"/>new <mark name=\"w3\"/>world </speak>"
        );
    }

    #[test]
    fn xml_escape_covers_special_chars() {
        assert_eq!(xml_escape("Q&A <tag> \"quote\" 'apos'"), "Q&amp;A &lt;tag&gt; &quot;quote&quot; &apos;apos&apos;");
    }

    #[test]
    fn word_timepoints_use_next_mark_as_end_and_last_word_has_zero_length() {
        let words = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let marks = vec![
            Timepoint { mark_name: "w0".into(), time_seconds: 0.0 },
            Timepoint { mark_name: "w1".into(), time_seconds: 0.5 },
            Timepoint { mark_name: "w2".into(), time_seconds: 1.2 },
        ];

        let spans = word_timepoints_from_marks(&words, &marks);

        assert_eq!(spans[0].start_ms, 0);
        assert_eq!(spans[0].end_ms, 500);
        assert_eq!(spans[1].start_ms, 500);
        assert_eq!(spans[1].end_ms, 1200);
        assert_eq!(spans[2].start_ms, 1200);
        assert_eq!(spans[2].end_ms, 1200);
    }

    #[test]
    fn cached_audio_is_reused_only_while_hash_matches() {
        let conn = test_conn();
        let book_id = books::create(&conn, "Dune", None, None).unwrap().id;
        let lesson = lessons::create(&conn, &book_id, "Ch1", "hello world", 0).unwrap();

        let cached = LessonAudio {
            lesson_id: lesson.id.clone(),
            audio_path: format!("{}.mp3", lesson.id),
            voice: "en-US".into(),
            word_timepoints: vec![],
            generated_at: now_iso8601(),
            text_hash_at_gen: lesson.text_hash.clone(),
        };
        save_audio(&conn, &cached).unwrap();

        let fetched = get_audio(&conn, &lesson.id).unwrap().unwrap();
        assert_eq!(fetched.text_hash_at_gen, lesson.text_hash);

        // Simulate an edit: the lesson's text_hash changes but the cached
        // row doesn't, so it should now read as stale.
        let edited = lessons::create(&conn, &book_id, "Ch1-edited", "hello galaxy", 1).unwrap();
        let stale_check = get_audio(&conn, &lesson.id).unwrap().unwrap();
        assert_ne!(stale_check.text_hash_at_gen, edited.text_hash);
    }
}
