use rusqlite::{params, Connection, Row};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::models::{Lesson, Question};
use crate::now_iso8601;

fn row_to_question(row: &Row) -> rusqlite::Result<Question> {
    Ok(Question {
        id: row.get(0)?,
        lesson_id: row.get(1)?,
        order_index: row.get(2)?,
        question_text: row.get(3)?,
        created_at: row.get(4)?,
    })
}

const SELECT_COLUMNS: &str = "id, lesson_id, order_index, question_text, created_at";

pub fn list_for_lesson(conn: &Connection, lesson_id: &str) -> Result<Vec<Question>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM questions WHERE lesson_id = ?1 ORDER BY order_index"
    ))?;
    let rows = stmt.query_map(params![lesson_id], row_to_question)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn save_all(conn: &Connection, lesson_id: &str, texts: &[String]) -> Result<Vec<Question>> {
    let now = now_iso8601();
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM questions WHERE lesson_id = ?1", params![lesson_id])?;

    let mut questions = Vec::with_capacity(texts.len());
    for (order_index, text) in texts.iter().enumerate() {
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO questions (id, lesson_id, order_index, question_text, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, lesson_id, order_index as i64, text, now],
        )?;
        questions.push(Question {
            id,
            lesson_id: lesson_id.to_string(),
            order_index: order_index as i64,
            question_text: text.clone(),
            created_at: now.clone(),
        });
    }
    tx.commit()?;
    Ok(questions)
}

#[derive(Deserialize)]
struct QuestionsPayload {
    questions: Vec<String>,
}

fn build_prompt(lesson_text: &str) -> String {
    format!(
        "You are writing reading comprehension questions for a language-learning student.\n\
         Read the lesson text below and write 3 to 5 open, free-response comprehension questions \
         about it. Do not include multiple-choice options or answers.\n\
         Respond with JSON only, in the exact form {{\"questions\": [\"question one\", \"question two\"]}}.\n\n\
         Lesson text:\n{lesson_text}"
    )
}

/// Ask Ollama (`{ollama_url}/api/generate`) for 3-5 open comprehension
/// questions about `lesson`'s text, replacing any previously stored
/// questions for that lesson. `api_key`, if set, is sent as a Bearer token
/// (needed for Ollama's hosted cloud API; a local Ollama server ignores it).
pub fn generate_questions(
    conn: &Connection,
    ollama_url: &str,
    api_key: Option<&str>,
    model: &str,
    lesson: &Lesson,
) -> Result<Vec<Question>> {
    let raw = crate::ollama::call_json(ollama_url, api_key, model, build_prompt(&lesson.text))?;
    let payload: QuestionsPayload = serde_json::from_str(&raw)
        .map_err(|e| CoreError::Ollama(format!("could not parse model response as JSON: {e} (raw: {raw})")))?;

    if payload.questions.is_empty() {
        return Err(CoreError::Ollama("model returned no questions".into()));
    }

     for i in 0..payload.questions.len() {
        println!("Question {}: {}", i + 1, payload.questions[i]);
    }
    save_all(conn, &lesson.id, &payload.questions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books;
    use crate::db::test_conn;
    use crate::lessons;

    #[test]
    fn save_all_replaces_existing_questions() {
        let conn = test_conn();
        let book_id = books::create(&conn, "Dune", None, None).unwrap().id;
        let lesson = lessons::create(&conn, &book_id, "Ch1", "hello world", 0).unwrap();

        save_all(&conn, &lesson.id, &["first?".to_string(), "second?".to_string()]).unwrap();
        let listed = list_for_lesson(&conn, &lesson.id).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].question_text, "first?");

        save_all(&conn, &lesson.id, &["only?".to_string()]).unwrap();
        let listed = list_for_lesson(&conn, &lesson.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].question_text, "only?");
    }

    #[test]
    fn build_prompt_includes_lesson_text() {
        let prompt = build_prompt("The quick fox.");
        assert!(prompt.contains("The quick fox."));
        assert!(prompt.contains("questions"));
    }
}
