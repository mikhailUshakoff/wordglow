use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
    format: &'static str,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

/// Call Ollama's `/api/generate` endpoint asking for a JSON-formatted
/// response, returning the raw `response` string for the caller to parse.
/// `api_key`, if set, is sent as a Bearer token (needed for Ollama's hosted
/// cloud API; a local Ollama server ignores it).
pub fn call_json(ollama_url: &str, api_key: Option<&str>, model: &str, prompt: String) -> Result<String> {
    let request = GenerateRequest {
        model,
        prompt,
        stream: false,
        format: "json",
    };

    let client = reqwest::blocking::Client::new();
    let mut builder = client
        .post(format!("{}/api/generate", ollama_url.trim_end_matches('/')))
        .json(&request);
    if let Some(api_key) = api_key {
        builder = builder.bearer_auth(api_key);
    }
    let response = builder.send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(CoreError::Ollama(format!("request failed ({status}): {body}")));
    }

    let parsed: GenerateResponse = response.json()?;
    Ok(parsed.response)
}

#[derive(Deserialize)]
struct TranslationPayload {
    translation: String,
}

fn build_translate_prompt(word: &str) -> String {
    format!(
        "Translate the word or short phrase \"{word}\" into Russian.\n\
         Respond with JSON only, in the exact form {{\"translation\": \"...\"}}"
         //containing just the Russian translation, no explanation.
    )
}

/// Ask Ollama to translate a single word or short phrase into Russian.
pub fn translate_word(ollama_url: &str, api_key: Option<&str>, model: &str, word: &str) -> Result<String> {
    let raw = call_json(ollama_url, api_key, model, build_translate_prompt(word))?;
    let payload: TranslationPayload = serde_json::from_str(&raw)
        .map_err(|e| CoreError::Ollama(format!("could not parse model response as JSON: {e} (raw: {raw})")))?;
    Ok(payload.translation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_translate_prompt_includes_word() {
        let prompt = build_translate_prompt("hello");
        assert!(prompt.contains("hello"));
        assert!(prompt.contains("Russian"));
    }
}
