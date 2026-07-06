//! Reverse-prompt (video→system-prompt) template and VL client call.
//!
//! The model is shown an ordered set of sampled frames and must return a JSON
//! object describing the clip across six film-language dimensions plus one fused,
//! reusable generation prompt. The six-dimension schema is inspired by the
//! MIT-licensed `raojiacui/prompt-lens` project; the wording here is original.
//!
//! The HTTP call uses the crate's existing blocking `reqwest` (no async runtime),
//! so it works in the plain CLI bin without pulling in tokio. It never touches
//! `video_url`; frames are sent as `image_url` data URLs, which works across
//! DashScope/Qwen, Gemini, GLM, and OpenAI OpenAI-compatible endpoints alike.

use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::runtime_config::P1ExternalConfig;
use crate::{Result, VideoAgentError};

/// System instruction that forces strict JSON output across the six dimensions
/// plus copy-extraction fields useful for subtitle/OCR and marketing reuse.
pub const SYSTEM_PROMPT: &str = r#"You are a senior video reverse-engineering analyst. You will be shown an ordered set of still frames sampled from ONE short video clip. Treat them as a single continuous piece.

Infer the visual recipe so the clip could be recreated as ORIGINAL content. Do not transcribe or reproduce any watermark, logo, or brand name verbatim. However, if the clip includes editorial subtitles, title cards, or meaningful on-screen copy that is part of the storytelling, extract it in normalized form.

Return ONLY a JSON object (no markdown fences, no commentary) with exactly these string keys:
- "subject":     the main character(s)/object(s): appearance, wardrobe, and action.
- "environment": location, setting, background, time of day, weather, key props.
- "camera":      shot sizes, angles, movement, lens feel, framing, and transitions.
- "lighting":    light sources, direction, color temperature, contrast, and light mood.
- "style":       overall aesthetic, genre, color grade, texture, era, render style.
- "mood":        emotional tone and pacing.
- "prompt":      ONE self-contained, reusable English text-to-video generation prompt
                 (60-120 words) that fuses the six dimensions to reproduce the STYLE
                 without copying the exact content.
- "subtitle_ocr": normalized Chinese subtitle/OCR text visible in the clip, deduplicated and ordered.
                  Include only meaningful story/editorial text, not watermarks. If none is visible,
                  return "none detected".
- "spoken_copy": the likely spoken line or concise talking-point sentence in Chinese. Prefer visible
                 subtitles if they clearly reveal the line; otherwise infer cautiously from the frames.
                 If there is no credible speech cue, return "none inferred".
- "promo_copy":  one concise Chinese promotional sentence (12-30 Chinese characters) suitable for
                 a cover line, teaser caption, or emotional hook derived from the clip's core message.

Every field must be a non-empty string. Base every claim on what is visible. If a transcript is provided in the user message, use it to improve the accuracy of spoken_copy and promo_copy, but do not let it override the visual description fields."#;

/// Keys every valid reverse-prompt artifact must carry as non-empty strings.
pub const REQUIRED_FIELDS: [&str; 11] = [
    "subject",
    "environment",
    "camera",
    "lighting",
    "style",
    "mood",
    "prompt",
    "subtitle_ocr",
    "audio_transcript",
    "spoken_copy",
    "promo_copy",
];

const VL_REQUIRED_FIELDS: [&str; 10] = [
    "subject",
    "environment",
    "camera",
    "lighting",
    "style",
    "mood",
    "prompt",
    "subtitle_ocr",
    "spoken_copy",
    "promo_copy",
];

/// Base64-encode raw JPEG bytes for embedding in an `image_url` data URL.
pub fn encode_frame(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Build the `[system, user]` chat messages, wrapping each base64 frame as an
/// `image_url` data URL. Input strings are raw base64 (not full data URLs).
pub fn build_vl_messages(frames_b64: &[String], audio_transcript: Option<&str>) -> Vec<Value> {
    let mut content = Vec::with_capacity(frames_b64.len() + 1);
    let intro = audio_transcript
        .map(str::trim)
        .filter(|transcript| !transcript.is_empty() && *transcript != "none available")
        .map(|transcript| {
            format!(
                "Here are {} frames sampled in order from one short video. Analyze them together and return the JSON described in the system prompt.\n\nASR transcript from the same clip audio (useful for spoken_copy and promo_copy):\n{}",
                frames_b64.len(),
                transcript
            )
        })
        .unwrap_or_else(|| {
            format!(
                "Here are {} frames sampled in order from one short video. Analyze them together and return the JSON described in the system prompt.",
                frames_b64.len()
            )
        });
    content.push(json!({
        "type": "text",
        "text": intro,
    }));
    for frame in frames_b64 {
        content.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:image/jpeg;base64,{frame}") },
        }));
    }
    vec![
        json!({ "role": "system", "content": SYSTEM_PROMPT }),
        json!({ "role": "user", "content": content }),
    ]
}

/// Resolved VL endpoint (OpenAI-compatible base URL + key + model).
#[derive(Debug, Clone)]
pub struct VlEndpoint {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl VlEndpoint {
    /// Build from resolved config, erroring if any of the three settings is unset.
    pub fn from_config(config: &P1ExternalConfig) -> Result<Self> {
        let take = |setting: &Option<crate::ResolvedSetting>, what: &str| {
            setting
                .as_ref()
                .map(|resolved| resolved.value.clone())
                .ok_or_else(|| {
                    VideoAgentError::Vl(format!(
                        "{what} is not configured (set it in ncx-config or via env)"
                    ))
                })
        };
        Ok(Self {
            api_key: take(&config.vl_api_key, "vl_api_key")?,
            base_url: take(&config.vl_base_url, "vl_base_url")?,
            model: take(&config.vl_model, "vl_model")?,
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

/// Call the VL model and return a validated reverse-prompt JSON object.
///
/// Reliability: JSON is forced via `response_format`, the parsed object is run
/// through [`crate::structured::validate_system_prompt_artifact`], and the whole
/// request is retried once before surfacing an error — so a caller either gets a
/// complete six-dimension artifact or a hard failure, never partial garbage.
pub fn request_reverse_prompt(
    endpoint: &VlEndpoint,
    frames_b64: &[String],
    audio_transcript: Option<&str>,
) -> Result<Value> {
    if frames_b64.is_empty() {
        return Err(VideoAgentError::Vl("no frames to send to the VL model".to_string()));
    }
    let body = json!({
        "model": endpoint.model,
        "messages": build_vl_messages(frames_b64, audio_transcript),
        "temperature": 0.2,
        "response_format": { "type": "json_object" },
        "max_tokens": 2800,
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| VideoAgentError::Vl(format!("build http client failed: {err}")))?;

    let mut last_error = String::new();
    for attempt in 1..=2 {
        match send_once(&client, endpoint, &body) {
            Ok(value) => {
                let reasons = validate_vl_payload(&value);
                if reasons.is_empty() {
                    return Ok(value);
                }
                last_error = format!("model output failed validation: {}", reasons.join("; "));
            }
            Err(err) => last_error = err,
        }
        eprintln!("VL attempt {attempt} failed: {last_error}");
    }
    Err(VideoAgentError::Vl(last_error))
}

fn send_once(
    client: &reqwest::blocking::Client,
    endpoint: &VlEndpoint,
    body: &Value,
) -> std::result::Result<Value, String> {
    let payload = serde_json::to_vec(body).map_err(|err| format!("serialize body failed: {err}"))?;
    let response = client
        .post(endpoint.chat_url())
        .header("Authorization", format!("Bearer {}", endpoint.api_key))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .map_err(|err| format!("request failed: {err}"))?;

    let status = response.status();
    let text = response
        .text()
        .map_err(|err| format!("read response body failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", truncate(&text, 500)));
    }

    let envelope: Value =
        serde_json::from_str(&text).map_err(|err| format!("response is not JSON: {err}"))?;
    let content = envelope
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| "response missing choices[0].message.content".to_string())?;

    let object_text = extract_json_object(content)
        .ok_or_else(|| format!("no JSON object found in content: {}", truncate(content, 300)))?;
    serde_json::from_str(&object_text).map_err(|err| format!("content JSON parse failed: {err}"))
}

/// Pull the outermost `{...}` out of model content, tolerating code fences and
/// stray prose around it.
fn extract_json_object(content: &str) -> Option<String> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    (end > start).then(|| content[start..=end].to_string())
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

fn validate_vl_payload(value: &Value) -> Vec<String> {
    let Some(obj) = value.as_object() else {
        return vec!["system prompt artifact must be a JSON object".to_string()];
    };
    let mut reasons = Vec::new();
    for field in VL_REQUIRED_FIELDS {
        if obj
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            reasons.push(format!("{field} must be a non-empty string"));
        }
    }
    reasons
}

/// System instruction for the dedicated high-resolution subtitle/OCR pass.
pub const OCR_SYSTEM_PROMPT: &str = r#"You are an OCR engine for short-video subtitles. You will be shown high-resolution frames from ONE short video. Read ONLY the burned-in editorial subtitle / caption / title-card text (the storytelling text), in its original language (usually Chinese).

Rules:
- Ignore watermarks, usernames, logos, hashtags, UI overlays, and platform stickers.
- Deduplicate lines that persist across frames; keep natural reading order.
- Do not translate, summarize, or invent text. Transcribe only what is legible.

Return ONLY a JSON object: {"subtitle_ocr": "<text>"}. Join distinct lines with " / ". If no editorial subtitle text is visible, return {"subtitle_ocr": "none detected"}."#;

/// Run a dedicated OCR pass over high-resolution frames and return the subtitle
/// text. Kept separate from the main VL call so OCR frames can be higher-res
/// (see [`crate::keyframes::OCR_MAX_WIDTH`]) without inflating the main request.
pub fn request_subtitle_ocr(endpoint: &VlEndpoint, frames_b64: &[String]) -> Result<String> {
    if frames_b64.is_empty() {
        return Err(VideoAgentError::Vl("no frames to send for OCR".to_string()));
    }
    let mut content = Vec::with_capacity(frames_b64.len() + 1);
    content.push(json!({
        "type": "text",
        "text": format!(
            "Read the subtitle text from these {} high-resolution frames and return the JSON described in the system prompt.",
            frames_b64.len()
        ),
    }));
    for frame in frames_b64 {
        content.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:image/jpeg;base64,{frame}") },
        }));
    }
    let body = json!({
        "model": endpoint.model,
        "messages": [
            { "role": "system", "content": OCR_SYSTEM_PROMPT },
            { "role": "user", "content": content },
        ],
        "temperature": 0.0,
        "response_format": { "type": "json_object" },
        "max_tokens": 800,
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| VideoAgentError::Vl(format!("build http client failed: {err}")))?;

    let mut last_error = String::new();
    for attempt in 1..=2 {
        match send_once(&client, endpoint, &body) {
            Ok(value) => {
                return Ok(value
                    .get("subtitle_ocr")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .unwrap_or("none detected")
                    .to_string());
            }
            Err(err) => last_error = err,
        }
        eprintln!("OCR attempt {attempt} failed: {last_error}");
    }
    Err(VideoAgentError::Vl(last_error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_vl_messages_has_system_and_all_image_parts() {
        let frames = vec!["AAAA".to_string(), "BBBB".to_string()];
        let messages = build_vl_messages(&frames, Some("你好，世界"));
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        let content = messages[1]["content"].as_array().unwrap();
        // 1 text part + 2 image parts.
        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"].as_str().unwrap().contains("你好，世界"));
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(
            content[1]["image_url"]["url"],
            "data:image/jpeg;base64,AAAA"
        );
    }

    #[test]
    fn extract_json_object_strips_fences_and_prose() {
        let content = "Sure, here you go:\n```json\n{\"subject\":\"cat\"}\n```\nHope that helps!";
        let extracted = extract_json_object(content).unwrap();
        let value: Value = serde_json::from_str(&extracted).unwrap();
        assert_eq!(value["subject"], "cat");
    }

    #[test]
    fn extract_json_object_none_when_absent() {
        assert!(extract_json_object("no json here").is_none());
    }

    #[test]
    fn from_config_errors_when_unset() {
        let config = P1ExternalConfig {
            ark_api_key: None,
            vl_api_key: None,
            vl_base_url: None,
            vl_model: None,
            config_error: None,
        };
        let err = VlEndpoint::from_config(&config).unwrap_err();
        assert!(matches!(err, VideoAgentError::Vl(_)));
    }

    #[test]
    fn encode_frame_roundtrips() {
        let encoded = encode_frame(&[0xFF, 0xD8, 0xFF]);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, vec![0xFF, 0xD8, 0xFF]);
    }
}
