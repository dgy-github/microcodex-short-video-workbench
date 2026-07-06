use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use reqwest::blocking::multipart::{Form, Part};
use serde_json::{json, Value};

use crate::media::validate_video_file_l0;
use crate::runtime_config::P1ExternalConfig;
use crate::{ffmpeg_command, ffprobe_command, ResolvedSetting, Result, VideoAgentError};

const DEFAULT_ASR_MODEL: &str = "qwen3-asr-flash";
const FILETRANS_ASR_MODEL: &str = "qwen3-asr-flash-filetrans";
const DEFAULT_TRANSCRIPT_PLACEHOLDER: &str = "none available";
const MAX_DATA_URI_AUDIO_BYTES: u64 = 19 * 1024 * 1024;
const MAX_INLINE_AUDIO_DURATION_SECS: f64 = 290.0;
const TRANSCRIPTION_SEGMENT_DURATION_SECS: u64 = 240;
const TRANSCRIPTION_AUDIO_SAMPLE_RATE: &str = "16000";
const TRANSCRIPTION_AUDIO_BITRATE: &str = "32k";
const FILETRANS_POLL_INTERVAL_SECS: u64 = 2;
const FILETRANS_MIN_WAIT_SECS: u64 = 300;
const FILETRANS_MAX_WAIT_SECS: u64 = 1800;

#[derive(Debug, Clone)]
pub struct TranscriptionSegment {
    pub id: String,
    pub start_sec: f64,
    pub end_sec: f64,
    pub text: String,
    pub speaker: Option<String>,
    pub confidence: Option<f64>,
    pub emotion: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TranscriptionArtifact {
    pub provider: String,
    pub transcript: String,
    pub segments: Vec<TranscriptionSegment>,
    pub raw_result: Option<Value>,
}

#[derive(Debug, Clone)]
struct PreparedTranscriptionAudio {
    path: PathBuf,
    cleanup: bool,
}

#[derive(Debug, Clone)]
struct UploadPolicy {
    upload_url: String,
    upload_dir: String,
    policy: String,
    signature: String,
    oss_access_key_id: String,
    x_oss_object_acl: String,
    x_oss_forbid_overwrite: String,
}

#[derive(Debug, Clone)]
pub struct AsrEndpoint {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AsrEndpoint {
    /// ASR reuses the VL DashScope endpoint + key (same vendor). The model is a
    /// fixed default (`qwen3-asr-flash`) rather than a separate config knob: this
    /// path serves one ASR model, so a dedicated `asr_*` config would only ever
    /// duplicate `vl_*`. To change the ASR model, edit `DEFAULT_ASR_MODEL`.
    pub fn from_config(config: &P1ExternalConfig) -> Result<Self> {
        let api_key = require(
            config.vl_api_key.as_ref(),
            "vl_api_key is not configured (ASR reuses the VL key)",
        )?;
        let base_url = require(
            config.vl_base_url.as_ref(),
            "vl_base_url is not configured (ASR reuses the VL endpoint)",
        )?;
        Ok(Self {
            api_key,
            base_url,
            model: DEFAULT_ASR_MODEL.to_string(),
        })
    }

    fn generation_url(&self) -> String {
        format!(
            "{}/api/v1/services/aigc/multimodal-generation/generation",
            self.service_root()
        )
    }

    fn filetrans_submit_url(&self) -> String {
        format!(
            "{}/api/v1/services/audio/asr/transcription",
            self.service_root()
        )
    }

    fn filetrans_task_url(&self, task_id: &str) -> String {
        format!("{}/api/v1/tasks/{task_id}", self.service_root())
    }

    fn upload_policy_url(&self) -> String {
        format!(
            "{}/api/v1/uploads?action=getPolicy&model={FILETRANS_ASR_MODEL}",
            self.service_root()
        )
    }

    fn service_root(&self) -> &str {
        let trimmed = self.base_url.trim_end_matches('/');
        trimmed
            .strip_suffix("/compatible-mode/v1")
            .or_else(|| trimmed.strip_suffix("/compatible-mode"))
            .or_else(|| trimmed.strip_suffix("/api/v1"))
            .or_else(|| trimmed.strip_suffix("/v1"))
            .unwrap_or(trimmed)
    }
}

pub fn transcribe_video_audio(video_path: impl AsRef<Path>, config: &P1ExternalConfig) -> Result<String> {
    let video_path = video_path.as_ref();
    let Some(audio_path) = extract_audio_wav(video_path)? else {
        return Ok(DEFAULT_TRANSCRIPT_PLACEHOLDER.to_string());
    };
    let endpoint = AsrEndpoint::from_config(config)?;
    let transcript = request_transcription(&endpoint, &audio_path);
    let _ = fs::remove_file(&audio_path);
    transcript
}

pub fn request_transcription_artifact(
    endpoint: &AsrEndpoint,
    audio_path: &Path,
) -> Result<TranscriptionArtifact> {
    let prepared = prepare_audio_for_transcription(audio_path)?;
    let result = request_transcription_prepared(endpoint, &prepared.path);
    if prepared.cleanup {
        let _ = fs::remove_file(&prepared.path);
    }
    result
}

pub fn request_transcription(endpoint: &AsrEndpoint, audio_path: &Path) -> Result<String> {
    request_transcription_artifact(endpoint, audio_path).map(|artifact| artifact.transcript)
}

fn request_transcription_prepared(
    endpoint: &AsrEndpoint,
    audio_path: &Path,
) -> Result<TranscriptionArtifact> {
    if audio_requires_filetrans(audio_path) {
        return request_transcription_filetrans(endpoint, audio_path)
            .or_else(|filetrans_error| fallback_to_segmented_transcription(endpoint, audio_path, filetrans_error));
    }

    let direct = request_transcription_inner(endpoint, audio_path);
    if let Err(VideoAgentError::Transcription(message)) = &direct {
        if transcription_error_suggests_filetrans(message) {
            return request_transcription_filetrans(endpoint, audio_path).or_else(|filetrans_error| {
                fallback_to_segmented_transcription(endpoint, audio_path, filetrans_error)
            });
        }
    }
    direct.map(|transcript| simple_transcription_artifact(endpoint.model.clone(), transcript))
}

fn request_transcription_inner(endpoint: &AsrEndpoint, audio_path: &Path) -> Result<String> {
    let audio = fs::read(audio_path).map_err(|err| {
        VideoAgentError::Transcription(format!(
            "read extracted audio {} failed: {err}",
            audio_path.display()
        ))
    })?;
    if audio.is_empty() {
        return Ok(DEFAULT_TRANSCRIPT_PLACEHOLDER.to_string());
    }
    let audio_data_uri = format!(
        "data:{};base64,{}",
        audio_mime_from_path(audio_path),
        base64::engine::general_purpose::STANDARD.encode(audio)
    );

    let body = json!({
        "model": endpoint.model,
        "input": {
            "messages": [
                {
                    "role": "system",
                    "content": [
                        {
                            "text": "Transcribe the spoken audio faithfully. Return only the transcript text in the original language with natural punctuation. Do not add commentary."
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "audio": audio_data_uri
                        }
                    ]
                }
            ]
        },
        "parameters": {
            "asr_options": {
                "enable_itn": false
            }
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|err| {
            VideoAgentError::Transcription(format!("build http client failed: {err}"))
        })?;
    let payload = serde_json::to_vec(&body).map_err(|err| {
        VideoAgentError::Transcription(format!("serialize transcription body failed: {err}"))
    })?;
    let response = client
        .post(endpoint.generation_url())
        .header("Authorization", format!("Bearer {}", endpoint.api_key))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .map_err(|err| VideoAgentError::Transcription(format!("request failed: {err}")))?;

    let status = response.status();
    let text = response
        .text()
        .map_err(|err| VideoAgentError::Transcription(format!("read response body failed: {err}")))?;
    if !status.is_success() {
        return Err(VideoAgentError::Transcription(format!(
            "HTTP {status}: {}",
            truncate(&text, 500)
        )));
    }

    let envelope: Value = serde_json::from_str(&text).map_err(|err| {
        VideoAgentError::Transcription(format!("response is not JSON: {err}"))
    })?;
    let transcript = envelope
        .get("output")
        .and_then(|output| output.get("choices"))
        .or_else(|| envelope.get("choices"))
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(extract_text_content)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| DEFAULT_TRANSCRIPT_PLACEHOLDER.to_string());

    Ok(transcript)
}

fn fallback_to_segmented_transcription(
    endpoint: &AsrEndpoint,
    audio_path: &Path,
    filetrans_error: VideoAgentError,
) -> Result<TranscriptionArtifact> {
    match request_transcription_segmented(endpoint, audio_path) {
        Ok(transcript) => Ok(simple_transcription_artifact(endpoint.model.clone(), transcript)),
        Err(segmented_error) => Err(VideoAgentError::Transcription(format!(
            "filetrans failed ({}) and segmented fallback failed ({})",
            filetrans_error,
            segmented_error
        ))),
    }
}

fn request_transcription_segmented(endpoint: &AsrEndpoint, audio_path: &Path) -> Result<String> {
    let segment_dir = temp_audio_dir();
    let segments = split_audio_for_transcription(audio_path, &segment_dir)?;

    let mut transcripts = Vec::new();
    for segment_path in &segments {
        let transcript = request_transcription_inner(endpoint, segment_path)?;
        let normalized = transcript.trim();
        if !normalized.is_empty() && normalized != DEFAULT_TRANSCRIPT_PLACEHOLDER {
            transcripts.push(normalized.to_string());
        }
    }

    let _ = fs::remove_dir_all(&segment_dir);
    if transcripts.is_empty() {
        return Ok(DEFAULT_TRANSCRIPT_PLACEHOLDER.to_string());
    }
    Ok(transcripts.join("\n"))
}

fn request_transcription_filetrans(
    endpoint: &AsrEndpoint,
    audio_path: &Path,
) -> Result<TranscriptionArtifact> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|err| {
            VideoAgentError::Transcription(format!("build http client failed: {err}"))
        })?;

    let policy = request_upload_policy(&client, endpoint)?;
    let file_url = upload_audio_for_filetrans(&client, &policy, audio_path)?;
    let task_id = submit_filetrans_task(&client, endpoint, &file_url)?;
    let task_result = poll_filetrans_task(&client, endpoint, &task_id, audio_path)?;
    let transcription_url = extract_transcription_url(&task_result)?;
    let raw_result = download_filetrans_result(&client, &transcription_url)?;
    parse_filetrans_transcription_result(&raw_result)
}

fn audio_requires_filetrans(audio_path: &Path) -> bool {
    audio_duration_exceeds_inline_limit(audio_path) || audio_size_exceeds_inline_limit(audio_path)
}

fn prepare_audio_for_transcription(audio_path: &Path) -> Result<PreparedTranscriptionAudio> {
    let metadata = fs::metadata(audio_path).map_err(|err| {
        VideoAgentError::Transcription(format!(
            "read audio metadata {} failed: {err}",
            audio_path.display()
        ))
    })?;
    if metadata.len() == 0 {
        return Ok(PreparedTranscriptionAudio {
            path: audio_path.to_path_buf(),
            cleanup: false,
        });
    }

    let extension = audio_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let should_transcode = matches!(extension.as_str(), "wav" | "wave")
        || metadata.len() > MAX_DATA_URI_AUDIO_BYTES;

    if !should_transcode {
        return Ok(PreparedTranscriptionAudio {
            path: audio_path.to_path_buf(),
            cleanup: false,
        });
    }

    let compressed_path = temp_audio_path("mp3");
    transcode_audio_for_transcription(audio_path, &compressed_path)?;
    let compressed_size = fs::metadata(&compressed_path)
        .map_err(|err| {
            VideoAgentError::Transcription(format!(
                "read compressed audio metadata {} failed: {err}",
                compressed_path.display()
            ))
        })?
        .len();
    if compressed_size == 0 {
        let _ = fs::remove_file(&compressed_path);
        return Ok(PreparedTranscriptionAudio {
            path: audio_path.to_path_buf(),
            cleanup: false,
        });
    }
    Ok(PreparedTranscriptionAudio {
        path: compressed_path,
        cleanup: true,
    })
}

fn transcode_audio_for_transcription(input_path: &Path, output_path: &Path) -> Result<()> {
    let output = ffmpeg_command()
        .args(["-y", "-v", "error"])
        .arg("-i")
        .arg(input_path)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            TRANSCRIPTION_AUDIO_SAMPLE_RATE,
            "-c:a",
            "libmp3lame",
            "-b:a",
            TRANSCRIPTION_AUDIO_BITRATE,
        ])
        .arg(output_path)
        .output()
        .map_err(|err| {
            VideoAgentError::Transcription(format!(
                "failed to launch ffmpeg for ASR audio compression: {err}"
            ))
        })?;
    if !output.status.success() {
        return Err(VideoAgentError::Transcription(format!(
            "ffmpeg ASR audio compression failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn request_upload_policy(
    client: &reqwest::blocking::Client,
    endpoint: &AsrEndpoint,
) -> Result<UploadPolicy> {
    let response = client
        .get(endpoint.upload_policy_url())
        .header("Authorization", format!("Bearer {}", endpoint.api_key))
        .send()
        .map_err(|err| VideoAgentError::Transcription(format!("upload policy request failed: {err}")))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| VideoAgentError::Transcription(format!("read upload policy response failed: {err}")))?;
    if !status.is_success() {
        return Err(VideoAgentError::Transcription(format!(
            "upload policy HTTP {status}: {}",
            truncate(&text, 500)
        )));
    }

    let payload: Value = serde_json::from_str(&text).map_err(|err| {
        VideoAgentError::Transcription(format!("upload policy response is not JSON: {err}"))
    })?;
    let data = payload
        .get("data")
        .ok_or_else(|| VideoAgentError::Transcription("upload policy response missing data".to_string()))?;
    Ok(UploadPolicy {
        upload_url: json_string_any(data, &["file_upload_url", "upload_host"])?,
        upload_dir: json_string(data, "upload_dir")?,
        policy: json_string(data, "policy")?,
        signature: json_string(data, "signature")?,
        oss_access_key_id: json_string(data, "oss_access_key_id")?,
        x_oss_object_acl: json_string(data, "x_oss_object_acl")?,
        x_oss_forbid_overwrite: json_string(data, "x_oss_forbid_overwrite")?,
    })
}

fn upload_audio_for_filetrans(
    client: &reqwest::blocking::Client,
    policy: &UploadPolicy,
    audio_path: &Path,
) -> Result<String> {
    let file_name = sanitized_upload_file_name(audio_path);
    let object_key = format!("{}/{}", policy.upload_dir.trim_end_matches('/'), file_name);
    let file_bytes = fs::read(audio_path).map_err(|err| {
        VideoAgentError::Transcription(format!(
            "read audio for filetrans upload {} failed: {err}",
            audio_path.display()
        ))
    })?;
    let file_part = Part::bytes(file_bytes)
        .file_name(file_name.clone())
        .mime_str(audio_mime_from_path(audio_path))
        .map_err(|err| {
            VideoAgentError::Transcription(format!(
                "build multipart audio part for {} failed: {err}",
                audio_path.display()
            ))
        })?;
    let form = Form::new()
        .text("OSSAccessKeyId", policy.oss_access_key_id.clone())
        .text("Signature", policy.signature.clone())
        .text("policy", policy.policy.clone())
        .text("key", object_key.clone())
        .text("x-oss-object-acl", policy.x_oss_object_acl.clone())
        .text(
            "x-oss-forbid-overwrite",
            policy.x_oss_forbid_overwrite.clone(),
        )
        .text("success_action_status", "200")
        .part("file", file_part);

    let response = client
        .post(&policy.upload_url)
        .multipart(form)
        .send()
        .map_err(|err| VideoAgentError::Transcription(format!("audio upload request failed: {err}")))?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    if !status.is_success() {
        return Err(VideoAgentError::Transcription(format!(
            "audio upload HTTP {status}: {}",
            truncate(text.trim(), 500)
        )));
    }

    Ok(format!("oss://{object_key}"))
}

fn submit_filetrans_task(
    client: &reqwest::blocking::Client,
    endpoint: &AsrEndpoint,
    file_url: &str,
) -> Result<String> {
    let body = json!({
        "model": FILETRANS_ASR_MODEL,
        "input": {
            "file_url": file_url,
        },
        "parameters": {
            "enable_words": false,
        }
    });
    let payload = serde_json::to_vec(&body).map_err(|err| {
        VideoAgentError::Transcription(format!("serialize filetrans request body failed: {err}"))
    })?;
    let response = client
        .post(endpoint.filetrans_submit_url())
        .header("Authorization", format!("Bearer {}", endpoint.api_key))
        .header("Content-Type", "application/json")
        .header("X-DashScope-Async", "enable")
        .header("X-DashScope-OssResourceResolve", "enable")
        .body(payload)
        .send()
        .map_err(|err| VideoAgentError::Transcription(format!("filetrans submit request failed: {err}")))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| VideoAgentError::Transcription(format!("read filetrans submit response failed: {err}")))?;
    if !status.is_success() {
        return Err(VideoAgentError::Transcription(format!(
            "filetrans submit HTTP {status}: {}",
            truncate(&text, 500)
        )));
    }

    let payload: Value = serde_json::from_str(&text).map_err(|err| {
        VideoAgentError::Transcription(format!("filetrans submit response is not JSON: {err}"))
    })?;
    payload
        .get("output")
        .and_then(|output| output.get("task_id"))
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| {
            VideoAgentError::Transcription(format!(
                "filetrans submit response missing task_id: {}",
                truncate(&text, 500)
            ))
        })
}

fn poll_filetrans_task(
    client: &reqwest::blocking::Client,
    endpoint: &AsrEndpoint,
    task_id: &str,
    audio_path: &Path,
) -> Result<Value> {
    let duration_secs = probe_audio_duration_seconds(audio_path).unwrap_or(MAX_INLINE_AUDIO_DURATION_SECS + 1.0);
    let timeout_secs = ((duration_secs.ceil() as u64).saturating_add(300))
        .clamp(FILETRANS_MIN_WAIT_SECS, FILETRANS_MAX_WAIT_SECS);
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let response = client
            .get(endpoint.filetrans_task_url(task_id))
            .header("Authorization", format!("Bearer {}", endpoint.api_key))
            .header("X-DashScope-Async", "enable")
            .send()
            .map_err(|err| VideoAgentError::Transcription(format!("filetrans poll request failed: {err}")))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|err| VideoAgentError::Transcription(format!("read filetrans poll response failed: {err}")))?;
        if !status.is_success() {
            return Err(VideoAgentError::Transcription(format!(
                "filetrans poll HTTP {status}: {}",
                truncate(&text, 500)
            )));
        }

        let payload: Value = serde_json::from_str(&text).map_err(|err| {
            VideoAgentError::Transcription(format!("filetrans poll response is not JSON: {err}"))
        })?;
        let task_status = payload
            .get("output")
            .and_then(|output| output.get("task_status"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        match task_status {
            "SUCCEEDED" => return Ok(payload),
            "PENDING" | "RUNNING" => {
                if Instant::now() >= deadline {
                    return Err(VideoAgentError::Transcription(format!(
                        "filetrans task {task_id} timed out after {timeout_secs} seconds"
                    )));
                }
                thread::sleep(Duration::from_secs(FILETRANS_POLL_INTERVAL_SECS));
            }
            "FAILED" => {
                let message = payload
                    .get("output")
                    .and_then(|output| output.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("message").and_then(Value::as_str))
                    .unwrap_or("unknown filetrans failure");
                return Err(VideoAgentError::Transcription(format!(
                    "filetrans task {task_id} failed: {}",
                    truncate(message, 500)
                )));
            }
            other => {
                return Err(VideoAgentError::Transcription(format!(
                    "filetrans task {task_id} returned unexpected status '{}'",
                    other
                )));
            }
        }
    }
}

fn extract_transcription_url(task_result: &Value) -> Result<String> {
    task_result
        .get("output")
        .and_then(|output| output.get("result"))
        .and_then(|result| result.get("transcription_url"))
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .or_else(|| {
            task_result
                .get("output")
                .and_then(|output| output.get("results"))
                .and_then(Value::as_array)
                .and_then(|results| results.first())
                .and_then(|result| result.get("transcription_url"))
                .and_then(Value::as_str)
                .map(|value| value.to_string())
        })
        .ok_or_else(|| {
            VideoAgentError::Transcription(format!(
                "filetrans task result missing transcription_url: {}",
                truncate(&task_result.to_string(), 500)
            ))
        })
}

fn download_filetrans_result(
    client: &reqwest::blocking::Client,
    transcription_url: &str,
) -> Result<Value> {
    let response = client
        .get(transcription_url)
        .send()
        .map_err(|err| VideoAgentError::Transcription(format!("download transcription result failed: {err}")))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| VideoAgentError::Transcription(format!("read transcription result response failed: {err}")))?;
    if !status.is_success() {
        return Err(VideoAgentError::Transcription(format!(
            "download transcription result HTTP {status}: {}",
            truncate(&text, 500)
        )));
    }
    serde_json::from_str(&text).map_err(|err| {
        VideoAgentError::Transcription(format!("transcription result is not JSON: {err}"))
    })
}

fn parse_filetrans_transcription_result(result: &Value) -> Result<TranscriptionArtifact> {
    let transcripts = collect_filetrans_transcripts(result);
    if transcripts.is_empty() {
        return Err(VideoAgentError::Transcription(format!(
            "filetrans result missing transcripts: {}",
            truncate(&result.to_string(), 500)
        )));
    }

    let mut transcript_parts = Vec::new();
    let mut segments = Vec::new();
    let mut segment_index = 1usize;

    for transcript in transcripts {
        if let Some(text) = transcript.get("text").and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                transcript_parts.push(trimmed.to_string());
            }
        }

        if let Some(sentences) = transcript.get("sentences").and_then(Value::as_array) {
            for sentence in sentences {
                let text = sentence
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let Some(text) = text else {
                    continue;
                };
                let start_sec = json_number_as_f64(sentence.get("begin_time")).unwrap_or(0.0) / 1000.0;
                let end_sec = json_number_as_f64(sentence.get("end_time")).unwrap_or(start_sec * 1000.0) / 1000.0;
                segments.push(TranscriptionSegment {
                    id: format!("asr_{segment_index:03}"),
                    start_sec,
                    end_sec: end_sec.max(start_sec),
                    text: text.to_string(),
                    speaker: sentence
                        .get("speaker_id")
                        .and_then(Value::as_i64)
                        .map(|value| value.to_string()),
                    confidence: json_number_as_f64(sentence.get("confidence")),
                    emotion: sentence
                        .get("emotion")
                        .and_then(Value::as_str)
                        .map(|value| value.to_string()),
                    language: sentence
                        .get("language")
                        .and_then(Value::as_str)
                        .map(|value| value.to_string()),
                });
                segment_index += 1;
            }
        }
    }

    if transcript_parts.is_empty() && !segments.is_empty() {
        transcript_parts = segments.iter().map(|segment| segment.text.clone()).collect();
    }
    let transcript = normalize_transcript_text(transcript_parts.join("\n"));

    Ok(TranscriptionArtifact {
        provider: FILETRANS_ASR_MODEL.to_string(),
        transcript,
        segments,
        raw_result: Some(result.clone()),
    })
}

fn collect_filetrans_transcripts<'a>(value: &'a Value) -> Vec<&'a Value> {
    if let Some(transcripts) = value.get("transcripts").and_then(Value::as_array) {
        return transcripts.iter().collect();
    }
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        return results
            .iter()
            .flat_map(collect_filetrans_transcripts)
            .collect();
    }
    if let Some(result) = value.get("result") {
        return collect_filetrans_transcripts(result);
    }
    if let Some(array) = value.as_array() {
        return array.iter().flat_map(collect_filetrans_transcripts).collect();
    }
    Vec::new()
}

fn simple_transcription_artifact(provider: String, transcript: String) -> TranscriptionArtifact {
    TranscriptionArtifact {
        provider,
        transcript: normalize_transcript_text(transcript),
        segments: Vec::new(),
        raw_result: None,
    }
}

fn normalize_transcript_text(text: String) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        DEFAULT_TRANSCRIPT_PLACEHOLDER.to_string()
    } else {
        trimmed.to_string()
    }
}

fn audio_duration_exceeds_inline_limit(audio_path: &Path) -> bool {
    probe_audio_duration_seconds(audio_path)
        .map(|duration| duration > MAX_INLINE_AUDIO_DURATION_SECS)
        .unwrap_or(false)
}

fn audio_size_exceeds_inline_limit(audio_path: &Path) -> bool {
    fs::metadata(audio_path)
        .map(|metadata| metadata.len() > MAX_DATA_URI_AUDIO_BYTES)
        .unwrap_or(false)
}

fn probe_audio_duration_seconds(audio_path: &Path) -> Result<f64> {
    let output = ffprobe_command()
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nokey=1:noprint_wrappers=1",
        ])
        .arg(audio_path)
        .output()
        .map_err(|err| {
            VideoAgentError::Transcription(format!(
                "failed to launch ffprobe for audio duration probe: {err}"
            ))
        })?;
    if !output.status.success() {
        return Err(VideoAgentError::Transcription(format!(
            "ffprobe audio duration probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    value.parse::<f64>().map_err(|err| {
        VideoAgentError::Transcription(format!(
            "parse audio duration '{}' failed: {err}",
            truncate(&value, 80)
        ))
    })
}

fn split_audio_for_transcription(audio_path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir).map_err(|err| {
        VideoAgentError::Transcription(format!(
            "create transcription segment dir {} failed: {err}",
            output_dir.display()
        ))
    })?;

    let output_pattern = output_dir.join("segment_%03d.mp3");
    let output = ffmpeg_command()
        .args(["-y", "-v", "error"])
        .arg("-i")
        .arg(audio_path)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            TRANSCRIPTION_AUDIO_SAMPLE_RATE,
            "-c:a",
            "libmp3lame",
            "-b:a",
            TRANSCRIPTION_AUDIO_BITRATE,
            "-f",
            "segment",
            "-segment_time",
        ])
        .arg(TRANSCRIPTION_SEGMENT_DURATION_SECS.to_string())
        .args(["-reset_timestamps", "1"])
        .arg(&output_pattern)
        .output()
        .map_err(|err| {
            VideoAgentError::Transcription(format!(
                "failed to launch ffmpeg for ASR audio segmentation: {err}"
            ))
        })?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(output_dir);
        return Err(VideoAgentError::Transcription(format!(
            "ffmpeg ASR audio segmentation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let mut segments = fs::read_dir(output_dir)
        .map_err(|err| {
            VideoAgentError::Transcription(format!(
                "read transcription segment dir {} failed: {err}",
                output_dir.display()
            ))
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("mp3"))
        .collect::<Vec<_>>();
    segments.sort();

    if segments.is_empty() {
        let _ = fs::remove_dir_all(output_dir);
        return Err(VideoAgentError::Transcription(format!(
            "ffmpeg did not produce any ASR audio segments for {}",
            audio_path.display()
        )));
    }

    Ok(segments)
}

fn transcription_error_suggests_filetrans(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("audio is too long")
        || normalized.contains("badrequest.toolarge")
        || normalized.contains("exceeded limit on max bytes")
}

fn audio_mime_from_path(audio_path: &Path) -> &'static str {
    match audio_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("m4a" | "mp4") => "audio/mp4",
        Some("ogg" | "opus") => "audio/ogg",
        Some("webm") => "audio/webm",
        _ => "audio/wav",
    }
}

fn sanitized_upload_file_name(audio_path: &Path) -> String {
    let stem = audio_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("audio");
    let extension = audio_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("mp3");
    let safe_stem = stem
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect::<String>();
    let safe_stem = safe_stem.trim_matches('_').trim().to_string();
    let safe_stem = if safe_stem.is_empty() {
        "audio".to_string()
    } else {
        safe_stem
    };
    format!(
        "{safe_stem}-{}.{extension}",
        std::process::id()
    )
}

fn json_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| {
            VideoAgentError::Transcription(format!(
                "response missing string field '{key}'"
            ))
        })
}

fn json_string_any(value: &Value, keys: &[&str]) -> Result<String> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(Value::as_str) {
            return Ok(found.to_string());
        }
    }
    Err(VideoAgentError::Transcription(format!(
        "response missing string fields {}",
        keys.join(", ")
    )))
}

fn json_number_as_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|v| v as f64))
        .or_else(|| value.as_u64().map(|v| v as f64))
        .or_else(|| value.as_str().and_then(|v| v.parse::<f64>().ok()))
}

fn require(setting: Option<&ResolvedSetting>, error: &str) -> Result<String> {
    setting
        .map(|resolved| resolved.value.clone())
        .ok_or_else(|| VideoAgentError::Transcription(error.to_string()))
}

fn extract_audio_wav(video_path: &Path) -> Result<Option<PathBuf>> {
    let report = validate_video_file_l0(video_path, None, 0.0, false)?;
    let probe = report.probe.ok_or_else(|| {
        VideoAgentError::Ffmpeg(format!(
            "media probe missing while extracting audio for {}",
            video_path.display()
        ))
    })?;
    if probe.audio_streams == 0 {
        return Ok(None);
    }

    let audio_path = temp_audio_path("wav");
    let output = ffmpeg_command()
        .args(["-y", "-v", "error"])
        .arg("-i")
        .arg(video_path)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            "-map",
            "0:a:0",
        ])
        .arg(&audio_path)
        .output()
        .map_err(|err| {
            VideoAgentError::Ffmpeg(format!(
                "failed to launch ffmpeg for audio extraction: {err}"
            ))
        })?;
    if !output.status.success() {
        return Err(VideoAgentError::Ffmpeg(format!(
            "ffmpeg audio extraction failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let size_bytes = fs::metadata(&audio_path)
        .map_err(|err| {
            VideoAgentError::Ffmpeg(format!(
                "read extracted audio metadata {} failed: {err}",
                audio_path.display()
            ))
        })?
        .len();
    if size_bytes == 0 {
        let _ = fs::remove_file(&audio_path);
        return Ok(None);
    }

    Ok(Some(audio_path))
}

fn extract_text_content(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let parts = content.as_array()?;
    let joined = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.get("transcript").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!joined.is_empty()).then_some(joined)
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max_chars).collect();
        out.push_str("...");
        out
    }
}

fn temp_audio_path(extension: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ncx-audio-{}-{nanos}.{extension}",
        std::process::id()
    ))
}

fn temp_audio_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ncx-audio-segments-{}-{nanos}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extract_text_content_supports_plain_string() {
        assert_eq!(
            extract_text_content(&json!("hello world")).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn extract_text_content_supports_openai_style_parts() {
        let content = json!([
            {"type": "output_text", "text": "第一句"},
            {"type": "output_text", "text": "第二句"}
        ]);
        assert_eq!(
            extract_text_content(&content).as_deref(),
            Some("第一句\n第二句")
        );
    }

    #[test]
    fn asr_endpoint_reuses_vl_settings() {
        let config = P1ExternalConfig {
            ark_api_key: None,
            vl_api_key: Some(ResolvedSetting {
                value: "vl-key".to_string(),
                source: "test".to_string(),
            }),
            vl_base_url: Some(ResolvedSetting {
                value: "https://example.com/v1".to_string(),
                source: "test".to_string(),
            }),
            vl_model: Some(ResolvedSetting {
                value: "qwen3-vl-plus".to_string(),
                source: "test".to_string(),
            }),
            config_error: None,
        };

        let endpoint = AsrEndpoint::from_config(&config).unwrap();
        assert_eq!(endpoint.api_key, "vl-key");
        assert_eq!(endpoint.base_url, "https://example.com/v1");
        assert_eq!(endpoint.model, DEFAULT_ASR_MODEL);
    }

    #[test]
    fn asr_endpoint_errors_without_vl_settings() {
        let config = P1ExternalConfig {
            ark_api_key: None,
            vl_api_key: None,
            vl_base_url: None,
            vl_model: None,
            config_error: None,
        };
        assert!(AsrEndpoint::from_config(&config).is_err());
    }

    #[test]
    fn audio_mime_detection_supports_mp3() {
        assert_eq!(
            audio_mime_from_path(Path::new("sample.mp3")),
            "audio/mpeg"
        );
        assert_eq!(
            audio_mime_from_path(Path::new("sample.wav")),
            "audio/wav"
        );
    }

    #[test]
    fn transcription_error_detects_audio_too_long() {
        assert!(transcription_error_suggests_filetrans(
            "HTTP 400: {\"message\":\"Internal Error.Algo.InvalidParameter: The audio is too long\"}"
        ));
        assert!(transcription_error_suggests_filetrans(
            "HTTP 400: {\"code\":\"BadRequest.TooLarge\",\"message\":\"Exceeded limit on max bytes per data-uri item\"}"
        ));
        assert!(!transcription_error_suggests_filetrans(
            "HTTP 400: {\"message\":\"some other error\"}"
        ));
    }

    #[test]
    fn parse_filetrans_transcription_result_extracts_sentences() {
        let value = json!({
            "file_url": "oss://example/audio.mp3",
            "properties": {
                "audio_format": "mp3"
            },
            "transcripts": [
                {
                    "channel_id": 0,
                    "text": "第一句。第二句。",
                    "sentences": [
                        {
                            "begin_time": 0,
                            "end_time": 1200,
                            "text": "第一句。",
                            "emotion": "neutral",
                            "language": "zh"
                        },
                        {
                            "begin_time": 1200,
                            "end_time": 2600,
                            "text": "第二句。",
                            "emotion": "neutral",
                            "language": "zh"
                        }
                    ]
                }
            ]
        });

        let artifact = parse_filetrans_transcription_result(&value).unwrap();
        assert_eq!(artifact.provider, FILETRANS_ASR_MODEL);
        assert_eq!(artifact.transcript, "第一句。第二句。");
        assert_eq!(artifact.segments.len(), 2);
        assert_eq!(artifact.segments[0].start_sec, 0.0);
        assert_eq!(artifact.segments[1].end_sec, 2.6);
    }
}
