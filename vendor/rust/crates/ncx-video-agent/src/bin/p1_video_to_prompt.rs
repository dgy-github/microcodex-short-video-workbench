//! P1 tool: distill a downloaded video into a reusable system prompt.
//!
//! Pipeline: probe (A) -> ffmpeg keyframes (B) -> one VL chat call (C) ->
//! validated six-dimension reverse-prompt JSON (D). Config is read exactly like
//! `p1_smoke`, but here the VL settings are actually used to issue a request.
//!
//! Usage:
//!   p1_video_to_prompt <video> [--frames N] [--no-transcribe] [--ocr-hires]
//!                       [--db PATH --project ID]
//!
//! With `--db` and `--project`, the validated artifact is persisted via the same
//! structured-artifact path as the rest of the crate.

use std::process::ExitCode;

use ncx_video_agent::{
    encode_frame, extract_keyframes, extract_keyframes_scaled, insert_project_artifact,
    record_structured_validation_if_pass, request_reverse_prompt, request_subtitle_ocr,
    transcribe_video_audio, validate_system_prompt_artifact, AgentArtifactKind, Database,
    P1ExternalConfig, VlEndpoint, OCR_MAX_WIDTH,
};
use serde_json::Value;

const DEFAULT_FRAMES: usize = 12;
const GATE_VERSION: &str = "p1-video-to-prompt";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let parsed = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: {} <video> [--frames N] [--no-transcribe] [--ocr-hires] [--db PATH --project ID]",
                args.first().map(String::as_str).unwrap_or("p1_video_to_prompt")
            );
            return ExitCode::FAILURE;
        }
    };

    let config = P1ExternalConfig::load();
    if let Some(err) = &config.config_error {
        eprintln!("warning: ncx-config load failed, falling back to env: {err}");
    }
    let endpoint = match VlEndpoint::from_config(&config) {
        Ok(endpoint) => endpoint,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let audio_transcript = if parsed.no_transcribe {
        eprintln!("skipping audio transcription (--no-transcribe)");
        "none available".to_string()
    } else {
        eprintln!("transcribing source audio ...");
        match transcribe_video_audio(&parsed.video, &config) {
            Ok(transcript) => transcript,
            Err(err) => {
                eprintln!("warning: audio transcription failed, continuing without it: {err}");
                "none available".to_string()
            }
        }
    };
    if audio_transcript != "none available" {
        eprintln!("audio transcript ready ({} chars)", audio_transcript.chars().count());
    }

    // (A + B) probe and extract keyframes.
    eprintln!(
        "extracting up to {} keyframes from {} ...",
        parsed.frames, parsed.video
    );
    let frames = match extract_keyframes(&parsed.video, parsed.frames) {
        Ok(frames) => frames,
        Err(err) => {
            eprintln!("keyframe extraction failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "extracted {} frames; calling VL model {} at {} ...",
        frames.len(),
        endpoint.model,
        endpoint.base_url
    );

    // (C + D) one VL call, validated reverse-prompt JSON.
    let frames_b64: Vec<String> = frames.iter().map(|frame| encode_frame(frame)).collect();
    let mut artifact =
        match request_reverse_prompt(&endpoint, &frames_b64, Some(&audio_transcript)) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("reverse prompt failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(obj) = artifact.as_object_mut() {
        obj.insert(
            "audio_transcript".to_string(),
            Value::String(audio_transcript.clone()),
        );
    }

    // Optional dedicated high-res OCR pass — overrides the VL call's subtitle_ocr
    // (768px frames are too small for reliable subtitle OCR).
    if parsed.ocr_hires {
        let ocr_frames = parsed.frames.clamp(1, 6);
        eprintln!(
            "running high-res OCR pass ({ocr_frames} frames @ {OCR_MAX_WIDTH}px) ..."
        );
        match extract_keyframes_scaled(&parsed.video, ocr_frames, OCR_MAX_WIDTH) {
            Ok(frames) => {
                let b64: Vec<String> = frames.iter().map(|frame| encode_frame(frame)).collect();
                match request_subtitle_ocr(&endpoint, &b64) {
                    Ok(text) => {
                        if let Some(obj) = artifact.as_object_mut() {
                            obj.insert("subtitle_ocr".to_string(), Value::String(text.clone()));
                        }
                        eprintln!("OCR subtitle: {text}");
                    }
                    Err(err) => {
                        eprintln!("warning: OCR pass failed, keeping VL subtitle_ocr: {err}")
                    }
                }
            }
            Err(err) => {
                eprintln!("warning: OCR keyframe extraction failed, keeping VL subtitle_ocr: {err}")
            }
        }
    }

    let report = validate_system_prompt_artifact(&artifact);
    if !report.passed {
        eprintln!(
            "validated artifact is incomplete: {}",
            report.reasons.join("; ")
        );
        return ExitCode::FAILURE;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&artifact).unwrap_or_else(|_| artifact.to_string())
    );
    if let Some(prompt) = artifact.get("prompt").and_then(Value::as_str) {
        eprintln!("\n--- reusable prompt ---\n{prompt}");
    }

    if let (Some(db_path), Some(project)) = (parsed.db.as_deref(), parsed.project.as_deref()) {
        match persist(db_path, project, &artifact) {
            Ok(id) => eprintln!("persisted artifact {id} to {db_path}"),
            Err(err) => {
                eprintln!("persist failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

struct Parsed {
    video: String,
    frames: usize,
    db: Option<String>,
    project: Option<String>,
    no_transcribe: bool,
    ocr_hires: bool,
}

fn parse_args(args: &[String]) -> std::result::Result<Parsed, String> {
    let mut video: Option<String> = None;
    let mut frames = DEFAULT_FRAMES;
    let mut db: Option<String> = None;
    let mut project: Option<String> = None;
    let mut no_transcribe = false;
    let mut ocr_hires = false;

    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--frames" => {
                let value = iter.next().ok_or("--frames requires a value")?;
                frames = value
                    .parse::<usize>()
                    .map_err(|_| format!("--frames must be a positive integer, got {value}"))?;
            }
            "--db" => db = Some(iter.next().ok_or("--db requires a path")?.clone()),
            "--project" => project = Some(iter.next().ok_or("--project requires an id")?.clone()),
            "--no-transcribe" => no_transcribe = true,
            "--ocr-hires" => ocr_hires = true,
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => {
                if video.is_some() {
                    return Err(format!("unexpected extra argument {other}"));
                }
                video = Some(other.to_string());
            }
        }
    }

    let video = video.ok_or("missing <video> argument")?;
    if db.is_some() != project.is_some() {
        return Err("--db and --project must be provided together".to_string());
    }
    Ok(Parsed {
        video,
        frames,
        db,
        project,
        no_transcribe,
        ocr_hires,
    })
}

fn persist(db_path: &str, project: &str, artifact: &Value) -> ncx_video_agent::Result<String> {
    let db = Database::open(db_path)?;
    // Create the project if it does not exist yet; ignore "already exists".
    let _ = db.create_project(project, 0.0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let id = format!("system_prompt_{nanos}");

    insert_project_artifact(
        db.connection(),
        &id,
        project,
        AgentArtifactKind::SystemPrompt,
        artifact,
    )?;
    let report = validate_system_prompt_artifact(artifact);
    record_structured_validation_if_pass(db.connection(), &id, GATE_VERSION, &report)?;
    Ok(id)
}
