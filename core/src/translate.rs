use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::error::{CoreError, Result};

const MYMEMORY_URL: &str = "https://api.mymemory.translated.net/get";

/// Source language used when a word's book has no usable language set.
pub const DEFAULT_SOURCE_LANG: &str = "en";

/// Source language for translating a word from `lesson_id`: the primary
/// subtag of its book's `language` (e.g. `"en-US"` → `"en"`). `None` if the
/// lesson doesn't exist or its book has no usable language set.
pub fn source_lang_for_lesson(conn: &Connection, lesson_id: &str) -> Result<Option<String>> {
    let language: Option<String> = conn
        .query_row(
            "SELECT b.language FROM lessons l JOIN books b ON b.id = l.book_id WHERE l.id = ?1",
            params![lesson_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(language.as_deref().and_then(primary_subtag))
}

/// `"en-US"` / `"en_US"` / `" EN "` → `"en"`. `None` unless the subtag is
/// 2–3 ASCII letters (an ISO 639 code), so free-form values like
/// `"English"` fall back to the default instead of breaking the request.
fn primary_subtag(language: &str) -> Option<String> {
    let subtag = language.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    ((2..=3).contains(&subtag.len()) && subtag.chars().all(|c| c.is_ascii_alphabetic())).then_some(subtag)
}

/// Translate a single word or short phrase into Russian via the free
/// MyMemory API (no key needed).
///
/// `source_lang` is an ISO 639-1 code like `"en"`. `email`, if set, is sent
/// as MyMemory's `de` parameter, which raises the anonymous daily quota
/// (~5k chars/day) to ~50k chars/day.
pub fn translate_word(source_lang: &str, email: Option<&str>, word: &str) -> Result<String> {
    let langpair = format!("{source_lang}|ru");
    let mut query = vec![("q", word), ("langpair", langpair.as_str())];
    if let Some(email) = email {
        query.push(("de", email));
    }

    let response = reqwest::blocking::Client::new().get(MYMEMORY_URL).query(&query).send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(CoreError::Translate(format!("request failed ({status}): {body}")));
    }

    parse_response(&response.json()?)
}

/// MyMemory returns HTTP 200 even for API-level errors (quota exhausted, bad
/// langpair), signalling them via `responseStatus` (a number or a numeric
/// string) and `responseDetails`.
fn parse_response(body: &Value) -> Result<String> {
    let status = match &body["responseStatus"] {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    };
    if status != Some(200) {
        let details = body["responseDetails"].as_str().unwrap_or("unknown error");
        return Err(CoreError::Translate(format!("MyMemory error ({status:?}): {details}")));
    }

    let text = body["responseData"]["translatedText"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| CoreError::Translate(format!("no translation in response: {body}")))?;
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn primary_subtag_normalizes_codes() {
        assert_eq!(primary_subtag("en-US").as_deref(), Some("en"));
        assert_eq!(primary_subtag("de_DE").as_deref(), Some("de"));
        assert_eq!(primary_subtag(" FR ").as_deref(), Some("fr"));
        assert_eq!(primary_subtag("English"), None);
        assert_eq!(primary_subtag(""), None);
    }

    #[test]
    fn source_lang_comes_from_book() {
        let conn = crate::db::test_conn();
        let book = crate::books::create(&conn, "B", None, Some("de-DE")).unwrap();
        let lesson = crate::lessons::create_appending(&conn, &book.id, "L", "Hallo Welt").unwrap();
        assert_eq!(source_lang_for_lesson(&conn, &lesson.id).unwrap().as_deref(), Some("de"));
        assert_eq!(source_lang_for_lesson(&conn, "missing").unwrap(), None);

        let no_lang = crate::books::create(&conn, "C", None, None).unwrap();
        let lesson = crate::lessons::create_appending(&conn, &no_lang.id, "L", "text").unwrap();
        assert_eq!(source_lang_for_lesson(&conn, &lesson.id).unwrap(), None);
    }

    #[test]
    fn parses_successful_response() {
        let body = json!({
            "responseData": { "translatedText": " привет ", "match": 1 },
            "responseStatus": 200,
            "responseDetails": ""
        });
        assert_eq!(parse_response(&body).unwrap(), "привет");
    }

    #[test]
    fn accepts_string_status() {
        let body = json!({
            "responseData": { "translatedText": "кот" },
            "responseStatus": "200"
        });
        assert_eq!(parse_response(&body).unwrap(), "кот");
    }

    #[test]
    fn reports_quota_error() {
        let body = json!({
            "responseData": { "translatedText": "MYMEMORY WARNING: YOU USED ALL AVAILABLE FREE TRANSLATIONS FOR TODAY" },
            "responseStatus": 429,
            "responseDetails": "MYMEMORY WARNING: YOU USED ALL AVAILABLE FREE TRANSLATIONS FOR TODAY"
        });
        let err = parse_response(&body).unwrap_err().to_string();
        assert!(err.contains("FREE TRANSLATIONS"));
    }

    #[test]
    fn rejects_empty_translation() {
        let body = json!({ "responseData": { "translatedText": "" }, "responseStatus": 200 });
        assert!(parse_response(&body).is_err());
    }
}
