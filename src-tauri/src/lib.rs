use ncx_video_agent::{
    encode_frame, extract_keyframes, extract_keyframes_scaled, request_reverse_prompt,
    request_subtitle_ocr, request_transcription, validate_video_file_l0, AsrEndpoint,
    P1ExternalConfig, VlEndpoint, OCR_MAX_WIDTH,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const APP_DIR_NAME: &str = "MicrocodeXShortVideo";
const SETTINGS_SCHEMA_VERSION: u32 = 1;
const JOBS_SCHEMA_VERSION: u32 = 1;
const USAGE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_DEEPSEEK_URL: &str = "https://api.deepseek.com/beta";
const DEFAULT_VISION_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_FLASH_MODEL: &str = "deepseek-chat";
const DEFAULT_PRO_MODEL: &str = "deepseek-v4-pro";
const DEFAULT_VISION_MODEL: &str = "qwen3-vl-plus";
const DEFAULT_ASR_MODEL: &str = "qwen3-asr-flash";
const DEFAULT_DOUYIN_DOWNLOADER_DIR: &str = r"D:\agent_prac\douyin-downloader";
const DOUYIN_DOWNLOADER_DIR_ENV: &str = "MICROCODEX_DOUYIN_DOWNLOADER_DIR";
const BUNDLED_RUNTIME_ROOT_ENV: &str = "MICROCODEX_BUNDLED_RUNTIME_ROOT";
const PORTABLE_ROOT_ENV: &str = "MICROCODEX_PORTABLE_ROOT";
const PORTABLE_MARKER_FILE: &str = "portable.mode";
const JOB_POLL_INTERVAL_MS: u64 = 900;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextPreset {
    model: String,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextPresetsFile {
    flash: TextPreset,
    pro: TextPreset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextProviderFile {
    default_tier: String,
    route_kind: String,
    api_key: String,
    custom_base_url: String,
    presets: TextPresetsFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisionProviderFile {
    api_key: String,
    model: String,
    base_url: String,
    allow_advanced_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BudgetFile {
    per_job_cny: f64,
    per_batch_cny: f64,
    block_when_over_budget: bool,
    flash_input_per_m_tokens_cny: f64,
    flash_output_per_m_tokens_cny: f64,
    pro_input_per_m_tokens_cny: f64,
    pro_output_per_m_tokens_cny: f64,
    vl_input_per_frame_cny: f64,
    vl_output_per_frame_cny: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitsFile {
    max_frames: u32,
    max_competitors: u32,
    max_transcription_minutes: u32,
    auto_ocr: bool,
    auto_asr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSettingsFile {
    text_provider: TextProviderFile,
    vision_provider: VisionProviderFile,
    budget: BudgetFile,
    limits: LimitsFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSettingsEnvelope {
    schema_version: u32,
    updated_at_ms: u64,
    settings: RuntimeSettingsFile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TextProviderView {
    default_tier: String,
    route_kind: String,
    has_api_key: bool,
    api_key_masked: String,
    custom_base_url: String,
    presets: TextPresetsFile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisionProviderView {
    has_api_key: bool,
    api_key_masked: String,
    model: String,
    base_url: String,
    allow_advanced_override: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSettingsView {
    schema_version: u32,
    updated_at_ms: u64,
    settings_path: String,
    text_provider: TextProviderView,
    vision_provider: VisionProviderView,
    budget: BudgetFile,
    limits: LimitsFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextProviderUpdate {
    default_tier: String,
    route_kind: String,
    text_api_key: String,
    custom_base_url: String,
    presets: TextPresetsFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisionProviderUpdate {
    vision_api_key: String,
    model: String,
    base_url: String,
    allow_advanced_override: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSettingsUpdate {
    text_provider: TextProviderUpdate,
    vision_provider: VisionProviderUpdate,
    budget: BudgetFile,
    limits: LimitsFile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentCheckItem {
    key: String,
    label: String,
    status: String,
    detail: String,
    action_hint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentHealthReport {
    checked_at_ms: u64,
    overall_status: String,
    ok_count: u32,
    warning_count: u32,
    missing_count: u32,
    helper_script_path: String,
    items: Vec<EnvironmentCheckItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSnapshot {
    pending_jobs: u32,
    running_jobs: u32,
    finished_jobs_today: u32,
    estimated_spend_today_cny: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EstimateJobRequest {
    mode: String,
    source_kind: String,
    duration_minutes: u32,
    frame_count: u32,
    competitor_count: u32,
    text_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EstimateJobResult {
    estimated_prompt_tokens: u32,
    estimated_completion_tokens: u32,
    estimated_vl_frames: u32,
    estimated_vl_calls: u32,
    estimated_text_calls: u32,
    estimated_cost_cny: f64,
    exceeds_job_budget: bool,
    effective_text_model: String,
    effective_text_base_url: String,
    effective_vision_model: String,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobRequest {
    name: String,
    mode: String,
    source_kind: String,
    source_value: String,
    duration_minutes: u32,
    frame_count: u32,
    competitor_count: u32,
    text_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRecord {
    id: String,
    name: String,
    mode: String,
    source_kind: String,
    source_value: String,
    status: String,
    stage_key: String,
    progress: u8,
    text_tier: String,
    duration_minutes: u32,
    frame_count: u32,
    competitor_count: u32,
    estimated_prompt_tokens: u32,
    estimated_completion_tokens: u32,
    estimated_vl_frames: u32,
    estimated_vl_calls: u32,
    estimated_text_calls: u32,
    estimated_cost_cny: f64,
    effective_text_model: String,
    effective_text_base_url: String,
    effective_vision_model: String,
    target_prompt_tokens: u32,
    target_completion_tokens: u32,
    target_cost_cny: f64,
    actual_prompt_tokens: u32,
    actual_completion_tokens: u32,
    actual_cost_cny: f64,
    created_at_ms: u64,
    updated_at_ms: u64,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    artifact_dir: String,
    notes: Vec<String>,
    error: Option<String>,
    stage_index: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobView {
    id: String,
    name: String,
    mode: String,
    source_kind: String,
    source_value: String,
    status: String,
    stage_key: String,
    progress: u8,
    text_tier: String,
    estimated_prompt_tokens: u32,
    estimated_completion_tokens: u32,
    estimated_total_tokens: u32,
    actual_prompt_tokens: u32,
    actual_completion_tokens: u32,
    actual_total_tokens: u32,
    estimated_cost_cny: f64,
    actual_cost_cny: f64,
    effective_text_model: String,
    effective_text_base_url: String,
    effective_vision_model: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    artifact_dir: String,
    material_pack_path: Option<String>,
    competitor_report_path: Option<String>,
    stage_log_path: String,
    notes: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobQueueFile {
    schema_version: u32,
    updated_at_ms: u64,
    next_seq: u64,
    jobs: Vec<JobRecord>,
}

struct JobStore {
    file_path: PathBuf,
    jobs_root: PathBuf,
    queue: JobQueueFile,
}

struct AppState {
    jobs: Arc<Mutex<JobStore>>,
}

#[derive(Debug, Clone)]
struct JobWorkItem {
    id: String,
    name: String,
    mode: String,
    source_kind: String,
    source_value: String,
    stage_key: String,
    text_tier: String,
    effective_text_model: String,
    effective_text_base_url: String,
    frame_count: u32,
    duration_minutes: u32,
    artifact_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSpec {
    #[serde(default)]
    kind: String,
    value: String,
    #[serde(default)]
    label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompetitorSourceBundle {
    primary: SourceSpec,
    #[serde(default)]
    competitors: Vec<SourceSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompetitorManifestSample {
    id: String,
    label: String,
    kind: String,
    source_value: String,
    artifact_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompetitorManifest {
    primary: CompetitorManifestSample,
    competitors: Vec<CompetitorManifestSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MaterialSpeakerProfile {
    #[serde(default)]
    persona: String,
    #[serde(default)]
    tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MaterialEditableScript {
    #[serde(default)]
    hook: String,
    #[serde(default)]
    body: Vec<String>,
    #[serde(default)]
    ending: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MaterialVideoPromptDraft {
    #[serde(default)]
    visual_brief: String,
    #[serde(default)]
    spoken_brief: String,
    #[serde(default)]
    reusable_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MaterialEvidenceRefs {
    #[serde(default)]
    vision_summary: String,
    #[serde(default)]
    transcript_structured: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MaterialPackFile {
    #[serde(default)]
    job_id: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    audience: String,
    #[serde(default)]
    speaker_profile: MaterialSpeakerProfile,
    #[serde(default)]
    core_message: Vec<String>,
    #[serde(default)]
    editable_script: MaterialEditableScript,
    #[serde(default)]
    title_candidates: Vec<String>,
    #[serde(default)]
    cover_copy_candidates: Vec<String>,
    #[serde(default)]
    promo_copy: Vec<String>,
    #[serde(default)]
    video_prompt_draft: MaterialVideoPromptDraft,
    #[serde(default)]
    evidence_refs: MaterialEvidenceRefs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompetitorMetricReport {
    key: String,
    label: String,
    summary: String,
    current_score: f64,
    competitor_score: f64,
    competitor_best_score: f64,
    current_note: String,
    benchmark_note: String,
    action: String,
    rewrite_hint: String,
    prompt_tweaks: Vec<String>,
    prompt_focus: String,
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompetitorReport {
    job_id: String,
    current_label: String,
    current_topic: String,
    competitor_count: usize,
    competitor_labels: Vec<String>,
    top_findings: Vec<String>,
    recommended_focus: String,
    recommended_tweaks: Vec<String>,
    metrics: Vec<CompetitorMetricReport>,
    generated_by_model: String,
    llm_usage: LlmUsage,
    generated_at_ms: u64,
}

#[derive(Debug, Clone)]
struct TextEndpoint {
    api_key: String,
    base_url: String,
    model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LlmUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatResponseChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatResponseChoice>,
    #[serde(default)]
    usage: LlmUsage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterialPromptRewriteRequest {
    base_prompt: String,
    text_tier: String,
    platform_label: String,
    version_label: String,
    focus_label: String,
    #[serde(default)]
    tweak_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MaterialPromptRewriteResult {
    prompt: String,
    generated_by_model: String,
    llm_usage: LlmUsage,
    cost_cny: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageEvent {
    id: String,
    feature: String,
    model: String,
    text_tier: String,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    cost_cny: f64,
    created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageLedgerFile {
    schema_version: u32,
    updated_at_ms: u64,
    events: Vec<UsageEvent>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn default_settings() -> RuntimeSettingsFile {
    hydrate_settings_from_external(RuntimeSettingsFile {
        text_provider: TextProviderFile {
            default_tier: "flash".to_string(),
            route_kind: "official".to_string(),
            api_key: String::new(),
            custom_base_url: String::new(),
            presets: TextPresetsFile {
                flash: TextPreset {
                    model: DEFAULT_FLASH_MODEL.to_string(),
                    base_url: DEFAULT_DEEPSEEK_URL.to_string(),
                },
                pro: TextPreset {
                    model: DEFAULT_PRO_MODEL.to_string(),
                    base_url: DEFAULT_DEEPSEEK_URL.to_string(),
                },
            },
        },
        vision_provider: VisionProviderFile {
            api_key: String::new(),
            model: DEFAULT_VISION_MODEL.to_string(),
            base_url: DEFAULT_VISION_URL.to_string(),
            allow_advanced_override: false,
        },
        budget: BudgetFile {
            per_job_cny: 15.0,
            per_batch_cny: 100.0,
            block_when_over_budget: true,
            flash_input_per_m_tokens_cny: 1.05,
            flash_output_per_m_tokens_cny: 2.10,
            pro_input_per_m_tokens_cny: 3.30,
            pro_output_per_m_tokens_cny: 6.60,
            vl_input_per_frame_cny: 0.035,
            vl_output_per_frame_cny: 0.012,
        },
        limits: LimitsFile {
            max_frames: 16,
            max_competitors: 5,
            max_transcription_minutes: 10,
            auto_ocr: true,
            auto_asr: true,
        },
    })
}

fn hydrate_settings_from_external(mut settings: RuntimeSettingsFile) -> RuntimeSettingsFile {
    let external = P1ExternalConfig::load();
    let seeded_vl_key = external
        .vl_api_key
        .as_ref()
        .map(|setting| setting.value.clone())
        .unwrap_or_default();
    let seeded_vl_base = external
        .vl_base_url
        .as_ref()
        .map(|setting| setting.value.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_VISION_URL.to_string());
    let seeded_vl_model = external
        .vl_model
        .as_ref()
        .map(|setting| setting.value.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_VISION_MODEL.to_string());
    if settings.vision_provider.api_key.trim().is_empty() {
        settings.vision_provider.api_key = seeded_vl_key;
    }
    if settings.vision_provider.base_url.trim().is_empty() {
        settings.vision_provider.base_url = seeded_vl_base;
    }
    if settings.vision_provider.model.trim().is_empty() {
        settings.vision_provider.model = seeded_vl_model;
    }
    settings
}

fn default_settings_envelope() -> RuntimeSettingsEnvelope {
    RuntimeSettingsEnvelope {
        schema_version: SETTINGS_SCHEMA_VERSION,
        updated_at_ms: now_ms(),
        settings: default_settings(),
    }
}

fn current_exe_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent().map(Path::to_path_buf)
}

fn portable_root_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var(PORTABLE_ROOT_ENV) {
        let path = PathBuf::from(value.trim());
        if path.is_dir() {
            return Some(path);
        }
    }

    let exe_dir = current_exe_dir()?;
    exe_dir
        .join(PORTABLE_MARKER_FILE)
        .is_file()
        .then_some(exe_dir)
}

fn app_config_dir() -> Result<PathBuf, String> {
    if let Some(root) = portable_root_dir() {
        return Ok(root.join("data"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Ok(PathBuf::from(appdata).join(APP_DIR_NAME));
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "cannot resolve APPDATA or HOME".to_string())?;
    Ok(PathBuf::from(home)
        .join("AppData")
        .join("Roaming")
        .join(APP_DIR_NAME))
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("settings.json"))
}

fn jobs_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("jobs.json"))
}

fn usage_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("usage.json"))
}

fn jobs_root_dir() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("jobs"))
}

fn mask_secret(value: &str) -> String {
    if value.trim().is_empty() {
        return "(unset)".to_string();
    }
    let tail: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("****{tail}")
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("create {} failed: {e}", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|e| format!("read dir {} failed: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry failed: {e}"))?;
        let source_path = entry.path();
        let target_path = to.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            ensure_parent(&target_path)?;
            fs::copy(&source_path, &target_path).map_err(|e| {
                format!(
                    "copy {} -> {} failed: {e}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn read_json_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("read {} failed: {e}", path.display()))
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    ensure_parent(path)?;
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| format!("write {} failed: {e}", path.display()))
}

fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    ensure_parent(path)?;
    fs::write(path, text).map_err(|e| format!("write {} failed: {e}", path.display()))
}

fn default_usage_ledger() -> UsageLedgerFile {
    UsageLedgerFile {
        schema_version: USAGE_SCHEMA_VERSION,
        updated_at_ms: now_ms(),
        events: Vec::new(),
    }
}

fn load_usage_ledger() -> Result<UsageLedgerFile, String> {
    let path = usage_file_path()?;
    if !path.exists() {
        let ledger = default_usage_ledger();
        write_json_file(&path, &ledger)?;
        return Ok(ledger);
    }

    let raw = read_json_file(&path)?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {} failed: {e}", path.display()))
}

fn save_usage_ledger(ledger: &UsageLedgerFile) -> Result<(), String> {
    let path = usage_file_path()?;
    write_json_file(&path, ledger)
}

fn append_usage_event(event: UsageEvent) -> Result<(), String> {
    let mut ledger = load_usage_ledger()?;
    ledger.events.push(event);
    if ledger.events.len() > 5000 {
        let drop_count = ledger.events.len().saturating_sub(5000);
        ledger.events.drain(0..drop_count);
    }
    ledger.updated_at_ms = now_ms();
    save_usage_ledger(&ledger)
}

fn usage_cost_today_cny() -> Result<f64, String> {
    let ledger = load_usage_ledger()?;
    let now = now_ms();
    let lookback = 24 * 60 * 60 * 1000;
    Ok(ledger
        .events
        .iter()
        .filter(|event| event.created_at_ms >= now.saturating_sub(lookback))
        .map(|event| event.cost_cny)
        .sum())
}

fn bootstrap_dir() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bootstrap"))
}

fn bundled_runtime_hint_path() -> Result<PathBuf, String> {
    Ok(bootstrap_dir()?.join("bundled_runtime_root.txt"))
}

fn bundled_downloader_dir() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bundled-douyin-downloader"))
}

fn is_valid_bundled_runtime_root(path: &Path) -> bool {
    path.join("python").join("python.exe").is_file()
        || path.join("ffmpeg").join("bin").join("ffmpeg.exe").is_file()
        || path.join("douyin-downloader").is_dir()
}

fn bundled_runtime_root() -> Option<PathBuf> {
    if let Ok(value) = std::env::var(BUNDLED_RUNTIME_ROOT_ENV) {
        let path = PathBuf::from(value.trim());
        if is_valid_bundled_runtime_root(&path) {
            return Some(path);
        }
    }

    let hint_path = bundled_runtime_hint_path().ok()?;
    let text = fs::read_to_string(hint_path).ok()?;
    let path = PathBuf::from(text.trim());
    is_valid_bundled_runtime_root(&path).then_some(path)
}

fn bundled_python_path() -> Option<PathBuf> {
    let path = bundled_runtime_root()?.join("python").join("python.exe");
    path.is_file().then_some(path)
}

fn bundled_ffmpeg_path() -> Option<PathBuf> {
    let path = bundled_runtime_root()?
        .join("ffmpeg")
        .join("bin")
        .join("ffmpeg.exe");
    path.is_file().then_some(path)
}

fn bundled_playwright_browsers_dir() -> Option<PathBuf> {
    let path = bundled_runtime_root()?.join("playwright-browsers");
    path.is_dir().then_some(path)
}

fn resolve_python_program() -> PathBuf {
    bundled_python_path().unwrap_or_else(|| PathBuf::from("python"))
}

fn resolve_ffmpeg_program() -> PathBuf {
    bundled_ffmpeg_path().unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn apply_python_command_env(command: &mut Command) {
    command.env("PYTHONUTF8", "1");
    if let Some(path) = bundled_playwright_browsers_dir() {
        command.env("PLAYWRIGHT_BROWSERS_PATH", path);
    }
}

fn ensure_downloader_config_template(downloader_dir: &Path) -> Result<(), String> {
    let config_path = downloader_dir.join("config.yml");
    if config_path.is_file() {
        return Ok(());
    }

    for candidate in [
        "config.example.yml",
        "config.example.yaml",
        "config.template.yml",
        "config.template.yaml",
    ] {
        let source = downloader_dir.join(candidate);
        if source.is_file() {
            copy_with_overwrite(&source, &config_path)?;
            return Ok(());
        }
    }
    Ok(())
}

fn detect_bundled_runtime_resource_root(app: &AppHandle) -> Option<PathBuf> {
    let resource_path = app
        .path()
        .resource_dir()
        .ok()?
        .join("bundle")
        .join("windows-runtime");
    if is_valid_bundled_runtime_root(&resource_path) {
        return Some(resource_path);
    }

    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("bundle-assets")
        .join("windows-runtime");
    is_valid_bundled_runtime_root(&dev_path).then_some(dev_path)
}

fn prepare_bundled_runtime(app: &AppHandle) -> Result<(), String> {
    let Some(runtime_root) = detect_bundled_runtime_resource_root(app) else {
        return Ok(());
    };

    std::env::set_var(BUNDLED_RUNTIME_ROOT_ENV, &runtime_root);
    write_text_file(
        &bundled_runtime_hint_path()?,
        &runtime_root.display().to_string(),
    )?;

    let source_downloader = runtime_root.join("douyin-downloader");
    if source_downloader.is_dir() {
        let target_downloader = bundled_downloader_dir()?;
        if !target_downloader.exists() {
            copy_dir_recursive(&source_downloader, &target_downloader)?;
        }
        ensure_downloader_config_template(&target_downloader)?;
    }
    Ok(())
}

fn windows_setup_script_path() -> Result<PathBuf, String> {
    Ok(bootstrap_dir()?.join("setup_operator_environment.ps1"))
}

fn windows_setup_script_text() -> String {
    format!(
        r#"$ErrorActionPreference = "Stop"

param(
  [string]$DownloaderDir = "",
  [string]$DownloaderRepoUrl = "",
  [switch]$RunCookieLogin
)

function Test-Command([string]$Name) {{
  return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}}

function Resolve-BundledRuntimeRoot() {{
  $hintPath = Join-Path $env:APPDATA "{app_dir}\bootstrap\bundled_runtime_root.txt"
  if (Test-Path $hintPath) {{
    $candidate = (Get-Content $hintPath -Raw).Trim()
    if ($candidate -and (Test-Path $candidate)) {{
      return $candidate
    }}
  }}
  return ""
}}

function Ensure-WingetInstall([string]$Id, [string]$Label) {{
  if (-not (Test-Command "winget")) {{
    Write-Warning "winget unavailable. Please install $Label manually."
    return
  }}
  Write-Host "Installing $Label ..."
  winget install --id $Id -e --accept-package-agreements --accept-source-agreements
}}

function Resolve-PythonCommand([string]$BundledRuntimeRoot) {{
  if ($BundledRuntimeRoot) {{
    $bundledPython = Join-Path $BundledRuntimeRoot "python\python.exe"
    if (Test-Path $bundledPython) {{
      return $bundledPython
    }}
  }}
  return "python"
}}

function Ensure-ConfigTemplate([string]$TargetDownloaderDir) {{
  $configPath = Join-Path $TargetDownloaderDir "config.yml"
  if (Test-Path $configPath) {{
    return
  }}

  foreach ($candidate in @("config.example.yml", "config.example.yaml", "config.template.yml", "config.template.yaml")) {{
    $source = Join-Path $TargetDownloaderDir $candidate
    if (Test-Path $source) {{
      Copy-Item $source $configPath -Force
      Write-Host "config.yml created from $candidate"
      return
    }}
  }}
}}

$BundledRuntimeRoot = Resolve-BundledRuntimeRoot
if (-not $DownloaderDir) {{
  if ($env:{env_name}) {{
    $DownloaderDir = $env:{env_name}
  }} elseif ($BundledRuntimeRoot -and (Test-Path (Join-Path $BundledRuntimeRoot "douyin-downloader"))) {{
    $DownloaderDir = Join-Path $env:APPDATA "{app_dir}\bundled-douyin-downloader"
  }} else {{
    $DownloaderDir = "{default_dir}"
  }}
}}

$PythonCmd = Resolve-PythonCommand $BundledRuntimeRoot
if ($BundledRuntimeRoot) {{
  $BundledFfmpegBin = Join-Path $BundledRuntimeRoot "ffmpeg\bin"
  if (Test-Path $BundledFfmpegBin) {{
    $env:PATH = "$BundledFfmpegBin;$env:PATH"
  }}
  $BundledPlaywright = Join-Path $BundledRuntimeRoot "playwright-browsers"
  if (Test-Path $BundledPlaywright) {{
    $env:PLAYWRIGHT_BROWSERS_PATH = $BundledPlaywright
  }}
}}

Write-Host "== MicrocodeX operator environment setup =="
Write-Host "DownloaderDir: $DownloaderDir"
if ($BundledRuntimeRoot) {{
  Write-Host "BundledRuntime: $BundledRuntimeRoot"
}}

if (-not (Test-Path $PythonCmd) -and -not (Test-Command "python")) {{
  Ensure-WingetInstall "Python.Python.3.12" "Python 3.12"
  $PythonCmd = "python"
}} else {{
  & $PythonCmd --version
}}

if (-not (Test-Command "ffmpeg")) {{
  Ensure-WingetInstall "Gyan.FFmpeg" "FFmpeg"
}} else {{
  ffmpeg -version | Select-Object -First 1
}}

if (-not (Test-Path $DownloaderDir)) {{
  if ($BundledRuntimeRoot -and (Test-Path (Join-Path $BundledRuntimeRoot "douyin-downloader"))) {{
    $bundledDownloader = Join-Path $BundledRuntimeRoot "douyin-downloader"
    Copy-Item $bundledDownloader $DownloaderDir -Recurse -Force
  }}

  if ($DownloaderRepoUrl -and -not (Test-Command "git")) {{
    Ensure-WingetInstall "Git.Git" "Git"
  }}

  if (-not (Test-Path $DownloaderDir) -and $DownloaderRepoUrl -and (Test-Command "git")) {{
    git clone $DownloaderRepoUrl $DownloaderDir
  }} elseif (-not (Test-Path $DownloaderDir)) {{
    New-Item -ItemType Directory -Force -Path $DownloaderDir | Out-Null
    Write-Warning "Downloader directory was missing. A folder was created, but you still need to place a ready douyin-downloader project there or rerun this script with -DownloaderRepoUrl."
  }}
}}

Push-Location $DownloaderDir

if (Test-Path ".\requirements.txt") {{
  & $PythonCmd -m pip install -r .\requirements.txt
}} elseif (Test-Path ".\pyproject.toml") {{
  & $PythonCmd -m pip install .
}} else {{
  Write-Warning "No requirements.txt or pyproject.toml found in downloader directory."
}}

& $PythonCmd -m pip install playwright
& $PythonCmd -m playwright install chromium

Ensure-ConfigTemplate $DownloaderDir

if ($RunCookieLogin -and (Test-Path ".\config.yml")) {{
  $env:PYTHONUTF8 = "1"
  & $PythonCmd -m tools.cookie_fetcher --config config.yml
}}

Pop-Location

Write-Host ""
Write-Host "Next steps:"
Write-Host "1. Confirm DeepSeek key in MicrocodeX settings."
Write-Host "2. Confirm Qwen VL / ASR key in MicrocodeX settings."
Write-Host "3. If needed, rerun this script with -RunCookieLogin."
Write-Host "4. Open the app and run the environment check again."
"#,
        app_dir = APP_DIR_NAME,
        env_name = DOUYIN_DOWNLOADER_DIR_ENV,
        default_dir = DEFAULT_DOUYIN_DOWNLOADER_DIR.replace('\\', "\\\\")
    )
}

fn ensure_windows_setup_script() -> Result<PathBuf, String> {
    let path = windows_setup_script_path()?;
    write_text_file(&path, &windows_setup_script_text())?;
    Ok(path)
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn prepare_command(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn run_command_output(command: &mut Command, label: &str) -> Result<Output, String> {
    prepare_command(command);
    command
        .output()
        .map_err(|e| format!("launch {label} failed: {e}"))
}

fn output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{}\n{}", stdout.trim(), stderr.trim())
        .trim()
        .to_string()
}

fn copy_with_overwrite(from: &Path, to: &Path) -> Result<(), String> {
    ensure_parent(to)?;
    if to.exists() {
        fs::remove_file(to).map_err(|e| format!("remove {} failed: {e}", to.display()))?;
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copy {} -> {} failed: {e}", from.display(), to.display()))
}

fn job_input_dir(job_root: &Path) -> PathBuf {
    job_root.join("input")
}

fn job_derived_dir(job_root: &Path) -> PathBuf {
    job_root.join("derived")
}

fn job_frames_dir(job_root: &Path) -> PathBuf {
    job_derived_dir(job_root).join("frames")
}

fn job_analysis_dir(job_root: &Path) -> PathBuf {
    job_root.join("analysis")
}

fn job_output_dir(job_root: &Path) -> PathBuf {
    job_root.join("output")
}

fn job_logs_dir(job_root: &Path) -> PathBuf {
    job_root.join("logs")
}

fn material_pack_path(job_root: &Path) -> PathBuf {
    job_output_dir(job_root).join("material_pack.json")
}

fn stage_log_path(job_root: &Path) -> PathBuf {
    job_logs_dir(job_root).join("stage_log.txt")
}

fn competitor_primary_root(job_root: &Path) -> PathBuf {
    job_root.join("current")
}

fn competitor_samples_root(job_root: &Path) -> PathBuf {
    job_root.join("competitors")
}

fn competitor_sample_root(job_root: &Path, index: usize) -> PathBuf {
    competitor_samples_root(job_root).join(format!("sample_{:02}", index + 1))
}

fn competitor_manifest_path(job_root: &Path) -> PathBuf {
    job_analysis_dir(job_root).join("competitor_manifest.json")
}

fn competitor_report_path(job_root: &Path) -> PathBuf {
    job_output_dir(job_root).join("competitor_report.json")
}

fn ensure_job_layout(job_root: &Path) -> Result<(), String> {
    for dir in [
        job_input_dir(job_root),
        job_derived_dir(job_root),
        job_frames_dir(job_root),
        job_analysis_dir(job_root),
        job_output_dir(job_root),
        job_logs_dir(job_root),
    ] {
        fs::create_dir_all(&dir).map_err(|e| format!("create {} failed: {e}", dir.display()))?;
    }
    Ok(())
}

fn append_stage_log(job_root: &Path, stage_key: &str, message: &str) -> Result<(), String> {
    let path = stage_log_path(job_root);
    ensure_parent(&path)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {} failed: {e}", path.display()))?;
    writeln!(file, "[{}] [{}] {}", now_ms(), stage_key, message.trim())
        .map_err(|e| format!("write {} failed: {e}", path.display()))
}

fn open_in_explorer(path: &Path, select_target: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        if select_target {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        prepare_command(&mut command);
        command
            .spawn()
            .map_err(|e| format!("open {} failed: {e}", path.display()))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err(format!(
        "opening paths is not supported on this OS: {}",
        path.display()
    ))
}

fn is_video_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "mkv" | "avi" | "webm")
    )
}

fn is_image_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp")
    )
}

fn is_audio_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "m4a" | "aac" | "wav")
    )
}

fn collect_files_recursive(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(root).map_err(|e| format!("read dir {} failed: {e}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry failed: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn find_first_matching_file(
    root: &Path,
    predicate: fn(&Path) -> bool,
) -> Result<Option<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files.into_iter().find(|path| predicate(path)))
}

fn resolve_douyin_downloader_dir() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var(DOUYIN_DOWNLOADER_DIR_ENV) {
        let path = PathBuf::from(value.trim());
        if path.is_dir() {
            ensure_downloader_config_template(&path)?;
            return Ok(path);
        }
    }
    if let Ok(path) = bundled_downloader_dir() {
        if path.is_dir() {
            ensure_downloader_config_template(&path)?;
            return Ok(path);
        }
    }
    let fallback = PathBuf::from(DEFAULT_DOUYIN_DOWNLOADER_DIR);
    if fallback.is_dir() {
        ensure_downloader_config_template(&fallback)?;
        Ok(fallback)
    } else {
        Err(format!(
            "Douyin downloader directory is missing. Provide a bundled runtime, set {DOUYIN_DOWNLOADER_DIR_ENV}, or create {}.",
            fallback.display()
        ))
    }
}

fn probe_command(
    program: &Path,
    args: &[&str],
    cwd: Option<&Path>,
    configure: Option<fn(&mut Command)>,
) -> Result<String, String> {
    let mut command = Command::new(program);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command.args(args);
    if let Some(configure_fn) = configure {
        configure_fn(&mut command);
    }
    let label = program.display().to_string();
    let output = run_command_output(&mut command, &label)?;
    if !output.status.success() {
        return Err(output_text(&output));
    }
    Ok(output_text(&output))
}

fn line_preview(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("ok")
        .to_string()
}

fn cookie_token_names(config_text: &str) -> Vec<&'static str> {
    ["ttwid", "msToken", "odin_tt", "passport_csrf_token"]
        .into_iter()
        .filter(|token| config_text.contains(token))
        .collect()
}

fn has_playwright_chromium_install() -> bool {
    if bundled_playwright_browsers_dir().is_some() {
        return true;
    }
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return false;
    };
    let root = PathBuf::from(local_app_data).join("ms-playwright");
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .any(|name| name.starts_with("chromium-"))
}

fn build_environment_report(
    settings: &RuntimeSettingsFile,
) -> Result<EnvironmentHealthReport, String> {
    let helper_script_path = ensure_windows_setup_script()?;
    let mut items = Vec::new();
    let bundled_runtime = bundled_runtime_root();

    items.push(EnvironmentCheckItem {
        key: "windows_platform".to_string(),
        label: "Windows 运行平台".to_string(),
        status: if cfg!(target_os = "windows") {
            "ok"
        } else {
            "missing"
        }
        .to_string(),
        detail: if cfg!(target_os = "windows") {
            "当前应用已在 Windows 上运行。".to_string()
        } else {
            "当前构建不是 Windows 运行环境。".to_string()
        },
        action_hint: "建议仅向 Windows 10/11 x64 机器交付此桌面版。".to_string(),
    });

    items.push(EnvironmentCheckItem {
        key: "webview2".to_string(),
        label: "WebView2 Runtime".to_string(),
        status: "ok".to_string(),
        detail: "当前桌面窗口已成功启动，可视为 WebView2 已可用。".to_string(),
        action_hint: "NSIS 安装器已配置 bootstrapper；离线环境建议预装 Evergreen Runtime。"
            .to_string(),
    });

    items.push(EnvironmentCheckItem {
        key: "portable_mode".to_string(),
        label: "便携模式".to_string(),
        status: if let Some(root) = portable_root_dir() {
            if root.join(PORTABLE_MARKER_FILE).is_file() {
                "ok"
            } else {
                "warn"
            }
        } else {
            "warn"
        }
        .to_string(),
        detail: portable_root_dir()
            .map(|root| {
                format!(
                    "当前运行在绿色便携模式下，数据目录为 {}。",
                    root.join("data").display()
                )
            })
            .unwrap_or_else(|| "当前是常规安装/开发模式，默认把数据写入 APPDATA。".to_string()),
        action_hint: "若要做绿色便携版，请从带 portable.mode 标记文件的目录启动程序。".to_string(),
    });

    items.push(EnvironmentCheckItem {
        key: "bundled_runtime".to_string(),
        label: "安装包内置运行包".to_string(),
        status: if bundled_runtime.is_some() { "ok" } else { "warn" }.to_string(),
        detail: bundled_runtime
            .as_ref()
            .map(|path| format!("当前安装包已附带离线运行包：{}。", path.display()))
            .unwrap_or_else(|| "当前运行实例未检测到安装包内置的 Python / FFmpeg / Playwright 资源。".to_string()),
        action_hint: "若要做到“客户只装一个安装包”，请在打包机先准备 bundle-assets/windows-runtime 再重新构建 NSIS 安装包。".to_string(),
    });

    let ffmpeg_program = resolve_ffmpeg_program();
    let ffmpeg_item = match probe_command(&ffmpeg_program, &["-version"], None, None) {
        Ok(text) => EnvironmentCheckItem {
            key: "ffmpeg".to_string(),
            label: "FFmpeg".to_string(),
            status: "ok".to_string(),
            detail: format!("已检测到 ffmpeg：{}（{}）。", line_preview(&text), ffmpeg_program.display()),
            action_hint: "无需处理。".to_string(),
        },
        Err(err) => EnvironmentCheckItem {
            key: "ffmpeg".to_string(),
            label: "FFmpeg".to_string(),
            status: "missing".to_string(),
            detail: format!("未检测到 ffmpeg：{}。", line_preview(&err)),
            action_hint: "优先使用带内置运行包的安装器；否则运行内置 PowerShell 引导脚本，或手动安装 FFmpeg 并加入 PATH。".to_string(),
        },
    };
    items.push(ffmpeg_item);

    let python_program = resolve_python_program();
    let python_item = match probe_command(
        &python_program,
        &["--version"],
        None,
        Some(apply_python_command_env),
    ) {
        Ok(text) => EnvironmentCheckItem {
            key: "python".to_string(),
            label: "Python 命令".to_string(),
            status: "ok".to_string(),
            detail: format!(
                "Python 可用：{}（{}）。",
                line_preview(&text),
                python_program.display()
            ),
            action_hint: "无需处理。".to_string(),
        },
        Err(err) => EnvironmentCheckItem {
            key: "python".to_string(),
            label: "Python 命令".to_string(),
            status: "missing".to_string(),
            detail: format!("当前无法直接执行 python：{}。", line_preview(&err)),
            action_hint:
                "优先使用带内置运行包的安装器；否则安装 Python，并确保命令行可直接运行 python。"
                    .to_string(),
        },
    };
    items.push(python_item);

    let downloader_dir_result = resolve_douyin_downloader_dir();
    match downloader_dir_result.as_ref() {
        Ok(path) => items.push(EnvironmentCheckItem {
            key: "douyin_downloader_dir".to_string(),
            label: "Douyin Downloader 目录".to_string(),
            status: "ok".to_string(),
            detail: format!("已找到目录：{}。", path.display()),
            action_hint: format!("如需改路径，可设置环境变量 {}。", DOUYIN_DOWNLOADER_DIR_ENV),
        }),
        Err(err) => items.push(EnvironmentCheckItem {
            key: "douyin_downloader_dir".to_string(),
            label: "Douyin Downloader 目录".to_string(),
            status: "missing".to_string(),
            detail: err.clone(),
            action_hint: "把准备好的 douyin-downloader 放到默认目录，或设置环境变量指向实际路径。"
                .to_string(),
        }),
    }

    if let Ok(downloader_dir) = downloader_dir_result.as_ref() {
        let config_path = downloader_dir.join("config.yml");
        if config_path.is_file() {
            let config_text = read_json_file(&config_path).or_else(|_| {
                fs::read_to_string(&config_path)
                    .map_err(|e| format!("read {} failed: {e}", config_path.display()))
            });
            match config_text {
                Ok(text) => {
                    let tokens = cookie_token_names(&text);
                    let status = if tokens.len() >= 2 { "ok" } else { "warn" };
                    items.push(EnvironmentCheckItem {
                        key: "douyin_config".to_string(),
                        label: "Downloader 配置与 Cookie".to_string(),
                        status: status.to_string(),
                        detail: if tokens.is_empty() {
                            format!(
                                "已找到 config.yml，但还没识别到常用 Cookie token：{}。",
                                config_path.display()
                            )
                        } else {
                            format!(
                                "已找到 config.yml，并识别到 Cookie token：{}。",
                                tokens.join(", ")
                            )
                        },
                        action_hint:
                            "若单条视频下载失败，先在 downloader 目录运行一次 cookie 登录流程。"
                                .to_string(),
                    });
                }
                Err(err) => items.push(EnvironmentCheckItem {
                    key: "douyin_config".to_string(),
                    label: "Downloader 配置与 Cookie".to_string(),
                    status: "warn".to_string(),
                    detail: err,
                    action_hint: "确认 config.yml 可读，并完成一次浏览器 Cookie 登录。".to_string(),
                }),
            }
        } else {
            items.push(EnvironmentCheckItem {
                key: "douyin_config".to_string(),
                label: "Downloader 配置与 Cookie".to_string(),
                status: "missing".to_string(),
                detail: format!("缺少 {}。", config_path.display()),
                action_hint: "先准备 config.yml，再执行一次 cookie 登录。".to_string(),
            });
        }

        let python_module_item = match probe_command(
            &python_program,
            &["-c", "import playwright; print('playwright-python-ok')"],
            Some(downloader_dir),
            Some(apply_python_command_env),
        ) {
            Ok(text) => EnvironmentCheckItem {
                key: "python_playwright".to_string(),
                label: "Python Playwright 依赖".to_string(),
                status: "ok".to_string(),
                detail: format!("Python 侧 Playwright 可导入：{}。", line_preview(&text)),
                action_hint: "无需处理。".to_string(),
            },
            Err(err) => EnvironmentCheckItem {
                key: "python_playwright".to_string(),
                label: "Python Playwright 依赖".to_string(),
                status: "missing".to_string(),
                detail: format!("Python 侧 Playwright 未就绪：{}。", line_preview(&err)),
                action_hint: "运行内置引导脚本，或在 downloader 目录安装 requirements 并执行 python -m pip install playwright。".to_string(),
            },
        };
        items.push(python_module_item);

        items.push(EnvironmentCheckItem {
            key: "playwright_chromium".to_string(),
            label: "Playwright Chromium 浏览器".to_string(),
            status: if has_playwright_chromium_install() {
                "ok"
            } else {
                "missing"
            }
            .to_string(),
            detail: if bundled_playwright_browsers_dir().is_some() {
                "已检测到安装包内置的 Playwright Chromium 浏览器。".to_string()
            } else if has_playwright_chromium_install() {
                "已检测到本机 Playwright Chromium 安装目录。".to_string()
            } else {
                "未检测到 Playwright Chromium 浏览器安装目录。".to_string()
            },
            action_hint: "运行内置引导脚本，或执行 python -m playwright install chromium。"
                .to_string(),
        });
    }

    items.push(EnvironmentCheckItem {
        key: "deepseek_key".to_string(),
        label: "DeepSeek 文本 Key".to_string(),
        status: if settings.text_provider.api_key.trim().is_empty() {
            "missing"
        } else {
            "ok"
        }
        .to_string(),
        detail: if settings.text_provider.api_key.trim().is_empty() {
            "当前还未填写 DeepSeek 文本 API Key。".to_string()
        } else {
            "DeepSeek 文本 API Key 已配置。".to_string()
        },
        action_hint: "在配置中心填写 DeepSeek Key。".to_string(),
    });

    items.push(EnvironmentCheckItem {
        key: "qwen_key".to_string(),
        label: "Qwen VL / ASR Key".to_string(),
        status: if settings.vision_provider.api_key.trim().is_empty() {
            "missing"
        } else {
            "ok"
        }
        .to_string(),
        detail: if settings.vision_provider.api_key.trim().is_empty() {
            "当前还未填写 Qwen VL / ASR API Key。".to_string()
        } else {
            "Qwen VL / ASR API Key 已配置。".to_string()
        },
        action_hint: "在配置中心填写 Qwen Key。".to_string(),
    });

    let ok_count = items.iter().filter(|item| item.status == "ok").count() as u32;
    let warning_count = items.iter().filter(|item| item.status == "warn").count() as u32;
    let missing_count = items.iter().filter(|item| item.status == "missing").count() as u32;
    let overall_status = if missing_count > 0 {
        "missing"
    } else if warning_count > 0 {
        "warn"
    } else {
        "ok"
    }
    .to_string();

    Ok(EnvironmentHealthReport {
        checked_at_ms: now_ms(),
        overall_status,
        ok_count,
        warning_count,
        missing_count,
        helper_script_path: helper_script_path.display().to_string(),
        items,
    })
}

fn trim_share_url_token(candidate: &str) -> &str {
    candidate.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\''
                | '“'
                | '”'
                | '<'
                | '>'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | ','
                | '，'
                | '。'
                | ';'
                | '；'
                | '!'
                | '！'
                | '?'
                | '？'
        )
    })
}

fn looks_like_douyin_url(candidate: &str) -> bool {
    let lower = candidate.to_ascii_lowercase();
    let starts_like_url = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("v.douyin.com/")
        || lower.starts_with("v.iesdouyin.com/")
        || lower.starts_with("www.douyin.com/")
        || lower.starts_with("douyin.com/")
        || lower.starts_with("iesdouyin.com/");

    starts_like_url && (lower.contains("douyin.com/") || lower.contains("iesdouyin.com/"))
}

fn extract_douyin_url_from_share_text(value: &str) -> Option<String> {
    for token in value.split_whitespace() {
        let token = trim_share_url_token(token);
        if token.is_empty() || !looks_like_douyin_url(token) {
            continue;
        }
        if token.starts_with("http://") || token.starts_with("https://") {
            return Some(token.to_string());
        }
        return Some(format!("https://{}", token.trim_start_matches('/')));
    }
    None
}

fn summarize_douyin_download_issue(output_text: &str) -> Option<String> {
    let lower = output_text.to_ascii_lowercase();
    if lower.contains("failed to parse url") {
        Some(
            "无法从输入内容里解析出抖音链接。请粘贴分享文案中的 https://... 链接，或直接粘贴抖音视频链接。"
                .to_string(),
        )
    } else if lower.contains("failed to resolve short url") {
        Some("抖音短链解析失败，请重新复制链接后再试。".to_string())
    } else if lower.contains("下载失败或链接无效") {
        Some("抖音下载失败，链接可能无效，或分享文案没有被正确解析。".to_string())
    } else {
        None
    }
}

fn latest_downloaded_video(root: &Path, started_at: SystemTime) -> Result<PathBuf, String> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files)?;
    let mut best_after: Option<(SystemTime, PathBuf)> = None;

    for path in files.into_iter().filter(|path| is_video_file(path)) {
        let modified = path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        if modified >= started_at
            && best_after
                .as_ref()
                .map(|(current, _)| modified > *current)
                .unwrap_or(true)
        {
            best_after = Some((modified, path));
        }
    }

    best_after
        .map(|(_, path)| path)
        .ok_or_else(|| format!("no newly downloaded video found under {}", root.display()))
}

fn ingest_local_video(source_value: &str, job_root: &Path) -> Result<Vec<String>, String> {
    ensure_job_layout(job_root)?;
    let source = PathBuf::from(source_value);
    if !source.is_file() {
        return Err(format!("local video does not exist: {}", source.display()));
    }
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .unwrap_or("mp4");
    let target = job_input_dir(job_root).join(format!("source_video.{ext}"));
    copy_with_overwrite(&source, &target)?;
    let metadata = json!({
        "platform": "local",
        "imported_at_ms": now_ms(),
        "original_path": path_string(&source),
        "source_video": path_string(&target),
    });
    write_json_file(
        &job_input_dir(job_root).join("source_metadata.json"),
        &metadata,
    )?;
    Ok(vec![format!("已导入本地视频 {}", source.display())])
}

fn ingest_douyin_video(source_value: &str, job_root: &Path) -> Result<Vec<String>, String> {
    ensure_job_layout(job_root)?;
    let downloader_dir = resolve_douyin_downloader_dir()?;
    let download_url = extract_douyin_url_from_share_text(source_value).ok_or_else(|| {
        "未从输入内容里找到可用的抖音链接。请粘贴分享文案中的 https://... 链接，或直接粘贴抖音视频链接。"
            .to_string()
    })?;
    let started_at = SystemTime::now();
    let mut command = Command::new(resolve_python_program());
    command
        .current_dir(&downloader_dir)
        .arg("run.py")
        .arg("-c")
        .arg("config.yml")
        .arg("-u")
        .arg(&download_url);
    apply_python_command_env(&mut command);
    let output = run_command_output(&mut command, "douyin downloader")?;
    let log_path = job_input_dir(job_root).join("download_log.txt");
    let download_output_text = output_text(&output);
    write_text_file(&log_path, &download_output_text)?;
    if !output.status.success() {
        return Err(format!(
            "douyin downloader failed: {}",
            download_output_text
        ));
    }
    if let Some(message) = summarize_douyin_download_issue(&download_output_text) {
        return Err(format!("{message}\n下载日志: {}", log_path.display()));
    }

    let downloaded_video = latest_downloaded_video(&downloader_dir.join("Downloaded"), started_at)?;
    let source_dir = downloaded_video.parent().ok_or_else(|| {
        format!(
            "downloaded file has no parent: {}",
            downloaded_video.display()
        )
    })?;
    let video_ext = downloaded_video
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("mp4");
    let copied_video = job_input_dir(job_root).join(format!("source_video.{video_ext}"));
    copy_with_overwrite(&downloaded_video, &copied_video)?;

    let mut source_dir_files = Vec::new();
    collect_files_recursive(source_dir, &mut source_dir_files)?;
    if let Some(metadata_file) = source_dir_files.iter().find(|path| {
        path.extension().and_then(|ext| ext.to_str()) == Some("json")
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .contains("_data")
    }) {
        copy_with_overwrite(
            metadata_file,
            &job_input_dir(job_root).join("source_metadata.json"),
        )?;
    } else {
        let metadata = json!({
            "platform": "douyin",
            "downloaded_at_ms": now_ms(),
            "source_input": source_value,
            "source_url": download_url,
            "downloaded_video": path_string(&downloaded_video),
        });
        write_json_file(
            &job_input_dir(job_root).join("source_metadata.json"),
            &metadata,
        )?;
    }
    if let Some(cover_file) = source_dir_files.iter().find(|path| is_image_file(path)) {
        let cover_ext = cover_file
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("jpg");
        copy_with_overwrite(
            cover_file,
            &job_input_dir(job_root).join(format!("source_cover.{cover_ext}")),
        )?;
    }
    if let Some(audio_file) = source_dir_files.iter().find(|path| is_audio_file(path)) {
        let audio_ext = audio_file
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("mp3");
        copy_with_overwrite(
            audio_file,
            &job_input_dir(job_root).join(format!("source_music.{audio_ext}")),
        )?;
    }

    write_json_file(
        &job_input_dir(job_root).join("download_manifest.json"),
        &json!({
            "source_input": source_value,
            "source_url": download_url,
            "downloaded_video": path_string(&downloaded_video),
            "source_dir": path_string(source_dir),
            "download_log": path_string(&log_path),
        }),
    )?;

    Ok(vec![format!(
        "抖音视频已下载并复制到 {}",
        copied_video.display()
    )])
}

fn source_video_path(job_root: &Path) -> Result<PathBuf, String> {
    let input_dir = job_input_dir(job_root);
    let Some(path) = find_first_matching_file(&input_dir, is_video_file)? else {
        return Err(format!(
            "source video is missing under {}",
            input_dir.display()
        ));
    };
    Ok(path)
}

fn build_vl_endpoint(settings: &RuntimeSettingsFile) -> Result<VlEndpoint, String> {
    if settings.vision_provider.api_key.trim().is_empty() {
        return Err("VL API key is missing in runtime settings.".to_string());
    }
    if settings.vision_provider.base_url.trim().is_empty() {
        return Err("VL base URL is missing in runtime settings.".to_string());
    }
    if settings.vision_provider.model.trim().is_empty() {
        return Err("VL model is missing in runtime settings.".to_string());
    }
    Ok(VlEndpoint {
        api_key: settings.vision_provider.api_key.clone(),
        base_url: settings.vision_provider.base_url.clone(),
        model: settings.vision_provider.model.clone(),
    })
}

fn build_asr_endpoint(settings: &RuntimeSettingsFile) -> Result<AsrEndpoint, String> {
    if settings.vision_provider.api_key.trim().is_empty() {
        return Err("ASR requires the VL API key, but it is missing.".to_string());
    }
    if settings.vision_provider.base_url.trim().is_empty() {
        return Err("ASR requires the VL base URL, but it is missing.".to_string());
    }
    Ok(AsrEndpoint {
        api_key: settings.vision_provider.api_key.clone(),
        base_url: settings.vision_provider.base_url.clone(),
        model: DEFAULT_ASR_MODEL.to_string(),
    })
}

fn build_text_endpoint(
    settings: &RuntimeSettingsFile,
    item: &JobWorkItem,
) -> Result<TextEndpoint, String> {
    if settings.text_provider.api_key.trim().is_empty() {
        return Err("Text API key is missing in runtime settings.".to_string());
    }

    let model = if !item.effective_text_model.trim().is_empty() {
        item.effective_text_model.clone()
    } else {
        effective_text_preset(settings, &item.text_tier)
            .model
            .clone()
    };
    let base_url = if !item.effective_text_base_url.trim().is_empty() {
        item.effective_text_base_url.clone()
    } else {
        effective_text_base_url(settings, &item.text_tier)
    };

    if model.trim().is_empty() {
        return Err("Text model is missing for this job.".to_string());
    }
    if base_url.trim().is_empty() {
        return Err("Text base URL is missing for this job.".to_string());
    }

    Ok(TextEndpoint {
        api_key: settings.text_provider.api_key.clone(),
        base_url,
        model,
    })
}

fn build_text_endpoint_for_tier(
    settings: &RuntimeSettingsFile,
    tier: &str,
) -> Result<TextEndpoint, String> {
    if settings.text_provider.api_key.trim().is_empty() {
        return Err("Text API key is missing in runtime settings.".to_string());
    }

    let model = effective_text_preset(settings, tier).model.clone();
    let base_url = effective_text_base_url(settings, tier);

    if model.trim().is_empty() {
        return Err("Text model is missing for this prompt rewrite.".to_string());
    }
    if base_url.trim().is_empty() {
        return Err("Text base URL is missing for this prompt rewrite.".to_string());
    }

    Ok(TextEndpoint {
        api_key: settings.text_provider.api_key.clone(),
        base_url,
        model,
    })
}

fn chat_completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

fn request_structured_competitor_report(
    endpoint: &TextEndpoint,
    payload: &Value,
) -> Result<(Value, LlmUsage), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("build text client failed: {e}"))?;
    let url = chat_completions_url(&endpoint.base_url);
    let body = json!({
        "model": endpoint.model,
        "temperature": 0.2,
        "max_tokens": 2600,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": "你是一名短视频竞品分析策略师。请根据输入证据输出严格 JSON，不要输出任何 JSON 以外的解释。结论必须面向可执行改写，并保持字段完整。"
            },
            {
                "role": "user",
                "content": payload.to_string()
            }
        ]
    });

    let response = client
        .post(url)
        .bearer_auth(&endpoint.api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("request DeepSeek competitor report failed: {e}"))?;
    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("read DeepSeek competitor report response failed: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "DeepSeek competitor report failed: HTTP {} {}",
            status, response_text
        ));
    }

    let parsed: ChatCompletionResponse = serde_json::from_str(&response_text).map_err(|e| {
        format!("parse DeepSeek completion envelope failed: {e}; body={response_text}")
    })?;
    let content = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "DeepSeek completion returned empty content.".to_string())?;
    let content_json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse DeepSeek structured JSON failed: {e}; content={content}"))?;
    Ok((content_json, parsed.usage))
}

fn request_material_prompt_rewrite(
    endpoint: &TextEndpoint,
    settings: &RuntimeSettingsFile,
    request: &MaterialPromptRewriteRequest,
) -> Result<MaterialPromptRewriteResult, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("build text client failed: {e}"))?;
    let url = chat_completions_url(&endpoint.base_url);
    let body = json!({
        "model": endpoint.model,
        "temperature": 0.35,
        "max_tokens": 2400,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": "你是一名中文短视频生成提示词优化器。你的任务是把输入的 seed_prompt 重写成一条更适合直接复制到视频生成工具的最终提示词。必须保持事实不变，只能增强结构、镜头语言、人物表达、字幕约束和平台适配度。必须全程输出中文，不能出现英文单词、英文字段名、JSON 以外解释、markdown 代码块或额外说明。只输出严格 JSON：{\"prompt\":\"...\"}。prompt 内容要可直接粘贴使用，语言自然、完整、专业。"
            },
            {
                "role": "user",
                "content": json!({
                    "task": "请根据所选平台、版本、优化目标和调优方向，重写最终中文视频提示词。",
                    "platform": request.platform_label,
                    "version": request.version_label,
                    "focus": request.focus_label,
                    "tweaks": request.tweak_labels,
                    "seed_prompt": request.base_prompt,
                }).to_string()
            }
        ]
    });

    let response = client
        .post(url)
        .bearer_auth(&endpoint.api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("request DeepSeek material prompt rewrite failed: {e}"))?;
    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("read DeepSeek material prompt rewrite response failed: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "DeepSeek material prompt rewrite failed: HTTP {} {}",
            status, response_text
        ));
    }

    let parsed: ChatCompletionResponse = serde_json::from_str(&response_text).map_err(|e| {
        format!("parse DeepSeek material prompt rewrite envelope failed: {e}; body={response_text}")
    })?;
    let content = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "DeepSeek material prompt rewrite returned empty content.".to_string())?;
    let prompt = serde_json::from_str::<Value>(&content)
        .ok()
        .and_then(|value| {
            value
                .get("prompt")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or(content)
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("DeepSeek material prompt rewrite returned an empty prompt.".to_string());
    }

    let cost_cny = text_usage_cost_cny(settings, &request.text_tier, &parsed.usage);
    let event = UsageEvent {
        id: format!("usage_{}", now_ms()),
        feature: "material_prompt_rewrite".to_string(),
        model: endpoint.model.clone(),
        text_tier: normalize_tier(&request.text_tier).to_string(),
        prompt_tokens: parsed.usage.prompt_tokens,
        completion_tokens: parsed.usage.completion_tokens,
        total_tokens: parsed.usage.total_tokens,
        cost_cny,
        created_at_ms: now_ms(),
    };
    if let Err(err) = append_usage_event(event) {
        eprintln!("append usage event failed: {err}");
    }

    Ok(MaterialPromptRewriteResult {
        prompt,
        generated_by_model: endpoint.model.clone(),
        llm_usage: parsed.usage,
        cost_cny,
    })
}

fn extract_audio_wav(source_video: &Path, output_path: &Path) -> Result<(), String> {
    ensure_parent(output_path)?;
    if output_path.exists() {
        fs::remove_file(output_path)
            .map_err(|e| format!("remove {} failed: {e}", output_path.display()))?;
    }
    let mut command = Command::new(resolve_ffmpeg_program());
    command
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(source_video)
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
        .arg(output_path);
    let output = run_command_output(&mut command, "ffmpeg audio extraction")?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg audio extraction failed: {}",
            output_text(&output)
        ));
    }
    Ok(())
}

fn read_json_value(path: &Path) -> Result<Value, String> {
    let text = read_json_file(path)?;
    serde_json::from_str(&text).map_err(|e| format!("parse {} failed: {e}", path.display()))
}

fn json_string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn split_text_lines(text: &str) -> Vec<String> {
    let normalized = text.replace(" / ", "\n");
    let mut out = Vec::new();
    for line in normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !out.iter().any(|existing: &String| existing == line) {
            out.push(line.to_string());
        }
    }
    out
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '。' | '！' | '？' | '!' | '?') {
            let sentence = current.trim();
            if !sentence.is_empty() {
                out.push(sentence.to_string());
            }
            current.clear();
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn first_sentence(text: &str) -> String {
    split_sentences(text)
        .into_iter()
        .next()
        .unwrap_or_else(|| text.trim().to_string())
}

fn first_n_sentences(text: &str, n: usize) -> String {
    split_sentences(text)
        .into_iter()
        .take(n)
        .collect::<Vec<_>>()
        .join("")
}

fn derive_title_candidates(topic: &str, promo_copy: &str, ocr_lines: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    for candidate in [
        topic.trim().to_string(),
        promo_copy.trim().to_string(),
        ocr_lines.first().cloned().unwrap_or_default(),
    ] {
        let candidate = candidate.trim();
        if !candidate.is_empty() && !candidates.iter().any(|existing| existing == candidate) {
            candidates.push(candidate.to_string());
        }
    }
    if candidates.is_empty() {
        candidates.push("短视频素材提炼结果".to_string());
    }
    candidates
}

fn derive_cover_candidates(promo_copy: &str, ocr_lines: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    for candidate in [
        promo_copy.trim().to_string(),
        ocr_lines.get(1).cloned().unwrap_or_default(),
        ocr_lines.first().cloned().unwrap_or_default(),
    ] {
        let candidate = candidate.trim();
        if !candidate.is_empty() && !candidates.iter().any(|existing| existing == candidate) {
            candidates.push(candidate.to_string());
        }
    }
    candidates
}

fn guess_source_kind(value: &str) -> &'static str {
    let trimmed = value.trim();
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.contains("douyin.com")
        || trimmed.contains("iesdouyin.com")
    {
        "douyin_url"
    } else {
        "local_video"
    }
}

fn normalize_source_spec(
    spec: SourceSpec,
    fallback_kind: &str,
    fallback_label: &str,
) -> Result<SourceSpec, String> {
    let value = spec.value.trim().to_string();
    if value.is_empty() {
        return Err("样本来源不能为空。".to_string());
    }
    let inferred = if spec.kind.trim().is_empty() {
        if fallback_kind.trim().is_empty() {
            guess_source_kind(&value)
        } else {
            normalize_source_kind(fallback_kind)
        }
    } else {
        normalize_source_kind(&spec.kind)
    };
    Ok(SourceSpec {
        kind: inferred.to_string(),
        value,
        label: if spec.label.trim().is_empty() {
            fallback_label.to_string()
        } else {
            spec.label.trim().to_string()
        },
    })
}

fn parse_competitor_bundle(
    raw: &str,
    primary_kind_hint: &str,
) -> Result<CompetitorSourceBundle, String> {
    let mut bundle: CompetitorSourceBundle =
        serde_json::from_str(raw).map_err(|e| format!("解析竞品样本输入失败：{e}"))?;
    bundle.primary = normalize_source_spec(bundle.primary, primary_kind_hint, "当前视频")?;
    let mut normalized = Vec::new();
    for (index, spec) in bundle.competitors.into_iter().enumerate() {
        let fallback_kind = guess_source_kind(&spec.value).to_string();
        normalized.push(normalize_source_spec(
            spec,
            &fallback_kind,
            &format!("竞品 {}", index + 1),
        )?);
    }
    bundle.competitors = normalized;
    Ok(bundle)
}

fn read_competitor_manifest(job_root: &Path) -> Result<CompetitorManifest, String> {
    let path = competitor_manifest_path(job_root);
    let raw = read_json_file(&path)?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {} failed: {e}", path.display()))
}

fn read_material_pack_file(path: &Path) -> Result<MaterialPackFile, String> {
    let raw = read_json_file(path)?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {} failed: {e}", path.display()))
}

fn sample_work_item(
    parent: &JobWorkItem,
    sample: &CompetitorManifestSample,
    stage_key: &str,
) -> JobWorkItem {
    JobWorkItem {
        id: format!("{}::{}", parent.id, sample.id),
        name: sample.label.clone(),
        mode: "extract".to_string(),
        source_kind: sample.kind.clone(),
        source_value: sample.source_value.clone(),
        stage_key: stage_key.to_string(),
        text_tier: parent.text_tier.clone(),
        effective_text_model: parent.effective_text_model.clone(),
        effective_text_base_url: parent.effective_text_base_url.clone(),
        frame_count: parent.frame_count,
        duration_minutes: parent.duration_minutes,
        artifact_dir: PathBuf::from(&sample.artifact_dir),
    }
}

fn append_parent_stage_log(item: &JobWorkItem, message: impl AsRef<str>) {
    let _ = append_stage_log(&item.artifact_dir, &item.stage_key, message.as_ref());
}

fn copy_if_exists(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_file() {
        copy_with_overwrite(from, to)?;
    }
    Ok(())
}

fn compact_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn preview_text(text: &str, fallback: &str, max_len: usize) -> String {
    let cleaned = compact_text(text);
    if cleaned.is_empty() {
        return fallback.to_string();
    }
    let chars = cleaned.chars().collect::<Vec<_>>();
    if chars.len() > max_len {
        chars.into_iter().take(max_len).collect::<String>() + "..."
    } else {
        cleaned
    }
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| text.contains(keyword))
}

fn average_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        round1(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn unique_non_empty(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn material_pack_search_text(pack: &MaterialPackFile) -> String {
    compact_text(
        &[
            pack.topic.clone(),
            pack.audience.clone(),
            pack.speaker_profile.persona.clone(),
            pack.speaker_profile.tone.clone(),
            pack.core_message.join(" "),
            pack.editable_script.hook.clone(),
            pack.editable_script.body.join(" "),
            pack.editable_script.ending.clone(),
            pack.title_candidates.join(" "),
            pack.cover_copy_candidates.join(" "),
            pack.promo_copy.join(" "),
            pack.video_prompt_draft.visual_brief.clone(),
            pack.video_prompt_draft.spoken_brief.clone(),
            pack.video_prompt_draft.reusable_prompt.clone(),
        ]
        .join(" "),
    )
}

fn score_clamped(value: f64) -> f64 {
    round1(value.clamp(0.0, 10.0))
}

fn competitor_metric_meta(
    key: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
) {
    match key {
        "hook" => (
            "开头钩子",
            "前 3 秒的问题感、反常识感和主题进入速度",
            "把解释型开头改成先冲突、后解释。",
            "balanced",
            &["clearer_hook"],
        ),
        "authority" => (
            "权威感线索",
            "专家身份、可信线索和首屏说服力",
            "把身份标签、专业背景和可信场景前置到首屏。",
            "balanced",
            &["stronger_authority"],
        ),
        "subtitles" => (
            "字幕策略",
            "单屏字数、关键词高亮和信息条安全区",
            "把字幕压成短句，保留 1 个关键词做高亮。",
            "balanced",
            &["safer_subtitles"],
        ),
        "rhythm" => (
            "口播节奏",
            "一句一锤点的讲解节奏和停顿层次",
            "按“结论 -> 原因 -> 机制 -> 落点”四拍重写。",
            "lip_sync",
            &["clearer_hook"],
        ),
        "background" => (
            "背景层次",
            "空间层次、道具细节和环境可信度",
            "补足书房感、道具层次和景深信息。",
            "visual",
            &["vivid_background"],
        ),
        "lighting" => (
            "布光质感",
            "主光、补光、肤色和整体通透感",
            "明确主光、补光、肤色和整体色调。",
            "visual",
            &["softer_lighting"],
        ),
        "framing" => (
            "镜头构图",
            "竖屏构图、机位稳定和字幕安全区",
            "锁定竖屏中近景，给字幕和信息条预留安全区。",
            "visual",
            &["safer_subtitles"],
        ),
        _ => (
            "人物表达",
            "口播状态、亲和力和可信感",
            "统一人物状态，减少念稿感，增强陪伴感。",
            "lip_sync",
            &["stronger_authority"],
        ),
    }
}

fn competitor_metric_note(key: &str, pack: &MaterialPackFile) -> String {
    match key {
        "hook" => format!(
            "当前开场：{}",
            preview_text(&pack.editable_script.hook, "还没有明确钩子", 24)
        ),
        "authority" => format!(
            "当前人设：{}",
            preview_text(
                &format!(
                    "{} {}",
                    pack.speaker_profile.persona, pack.speaker_profile.tone
                ),
                "还没有明确的人设标签",
                26
            )
        ),
        "subtitles" => format!(
            "当前短句：{}",
            preview_text(
                &unique_non_empty(
                    pack.cover_copy_candidates
                        .iter()
                        .chain(pack.title_candidates.iter())
                        .take(2)
                        .cloned()
                        .collect::<Vec<_>>(),
                )
                .join(" / "),
                "还没有提炼出短字幕",
                28
            )
        ),
        "rhythm" => format!(
            "当前结构：正文 {} 段，核心 {} 点",
            pack.editable_script.body.len(),
            pack.core_message.len()
        ),
        "background" => format!(
            "当前背景：{}",
            preview_text(&pack.video_prompt_draft.visual_brief, "背景信息还偏少", 28)
        ),
        "lighting" => format!(
            "当前布光：{}",
            preview_text(
                &pack.video_prompt_draft.visual_brief,
                "布光描述还不够明确",
                28
            )
        ),
        "framing" => format!(
            "当前构图：{}",
            preview_text(
                &pack.video_prompt_draft.reusable_prompt,
                "机位约束还不够明确",
                28
            )
        ),
        _ => format!(
            "当前状态：{}",
            preview_text(
                &format!(
                    "{} {} {}",
                    pack.speaker_profile.persona,
                    pack.speaker_profile.tone,
                    pack.video_prompt_draft.spoken_brief
                ),
                "人物表达还偏泛",
                28
            )
        ),
    }
}

fn score_competitor_metric(key: &str, pack: &MaterialPackFile) -> f64 {
    match key {
        "hook" => {
            let hook = compact_text(&pack.editable_script.hook);
            if hook.is_empty() {
                return 5.6;
            }
            let mut score = 6.1;
            if contains_any(
                &hook,
                &["为什么", "怎么", "反而", "却", "竟然", "别再", "少吃"],
            ) {
                score += 1.1;
            }
            if hook.contains('？') || hook.contains('?') {
                score += 0.6;
            }
            if hook.chars().count() <= 18 {
                score += 0.5;
            }
            if contains_any(&hook, &["很多人", "今天", "我们"]) {
                score -= 0.3;
            }
            score_clamped(score)
        }
        "authority" => {
            let context = material_pack_search_text(pack);
            let mut score = 6.0;
            if contains_any(
                &context,
                &[
                    "专家", "教授", "博士", "医生", "学者", "剑桥", "复旦", "科普", "讲师",
                ],
            ) {
                score += 1.5;
            }
            if contains_any(&context, &["眼镜", "领带", "书房", "书架", "专业", "身份"])
            {
                score += 0.7;
            }
            if pack.speaker_profile.persona.chars().count() > 8 {
                score += 0.4;
            }
            score_clamped(score)
        }
        "subtitles" => {
            let candidates = unique_non_empty(
                pack.cover_copy_candidates
                    .iter()
                    .chain(pack.title_candidates.iter())
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            if candidates.is_empty() {
                return 5.9;
            }
            let shortest = candidates
                .iter()
                .map(|value| compact_text(value).chars().count())
                .min()
                .unwrap_or(20);
            let mut score = 6.0;
            if shortest <= 12 {
                score += 1.0;
            } else if shortest <= 16 {
                score += 0.6;
            } else if shortest >= 22 {
                score -= 0.4;
            }
            if contains_any(
                &candidates.join(" "),
                &["胖", "压力", "代谢", "胰岛素", "长寿"],
            ) {
                score += 0.5;
            }
            score_clamped(score + 0.4)
        }
        "rhythm" => {
            let body_count = pack.editable_script.body.len();
            let core_count = pack.core_message.len();
            let mut score = 6.1;
            if (2..=4).contains(&body_count) {
                score += 1.1;
            } else if body_count == 1 {
                score += 0.3;
            } else if body_count >= 5 {
                score -= 0.5;
            }
            if (2..=4).contains(&core_count) {
                score += 0.5;
            }
            if contains_any(
                &material_pack_search_text(pack),
                &["节奏", "停顿", "口播", "自然", "连贯"],
            ) {
                score += 0.3;
            }
            score_clamped(score)
        }
        "background" => {
            let visual = compact_text(&format!(
                "{} {}",
                pack.video_prompt_draft.visual_brief, pack.video_prompt_draft.reusable_prompt
            ));
            if visual.is_empty() {
                return 5.8;
            }
            let mut score = 5.9;
            if contains_any(
                &visual,
                &["书架", "窗帘", "木质", "书房", "景深", "背景", "道具"],
            ) {
                score += 1.3;
            }
            if contains_any(&visual, &["层次", "虚化", "环境", "高级", "安静"]) {
                score += 0.8;
            }
            score_clamped(score)
        }
        "lighting" => {
            let visual = compact_text(&format!(
                "{} {}",
                pack.video_prompt_draft.visual_brief, pack.video_prompt_draft.reusable_prompt
            ));
            if visual.is_empty() {
                return 6.0;
            }
            let mut score = 6.2;
            if contains_any(&visual, &["柔和", "主光", "补光", "肤色", "暖中性", "通透"])
            {
                score += 1.2;
            }
            if contains_any(&visual, &["布光", "高级", "自然", "不过曝", "不过灰"]) {
                score += 0.7;
            }
            score_clamped(score)
        }
        "framing" => {
            let visual = compact_text(&format!(
                "{} {}",
                pack.video_prompt_draft.visual_brief, pack.video_prompt_draft.reusable_prompt
            ));
            if visual.is_empty() {
                return 6.1;
            }
            let mut score = 6.3;
            if contains_any(
                &visual,
                &["9:16", "竖屏", "中近景", "胸像", "平视", "稳定", "机位"],
            ) {
                score += 1.4;
            }
            if contains_any(&visual, &["构图", "安全区", "字幕", "口型"]) {
                score += 0.5;
            }
            score_clamped(score)
        }
        _ => {
            let context = compact_text(&format!(
                "{} {} {}",
                pack.speaker_profile.persona,
                pack.speaker_profile.tone,
                pack.video_prompt_draft.spoken_brief
            ));
            if context.is_empty() {
                return 6.2;
            }
            let mut score = 6.3;
            if contains_any(
                &context,
                &["专业", "亲和", "沉稳", "可信", "自然", "克制", "讲解"],
            ) {
                score += 1.2;
            }
            if contains_any(&context, &["口播", "表达", "稳定", "陪伴"]) {
                score += 0.6;
            }
            if !pack.speaker_profile.persona.trim().is_empty() {
                score += 0.4;
            }
            score_clamped(score)
        }
    }
}

fn rewrite_hint_for_metric(key: &str, current_pack: &MaterialPackFile, best_label: &str) -> String {
    match key {
        "hook" => {
            let candidate = if current_pack.editable_script.hook.trim().is_empty() {
                current_pack
                    .title_candidates
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "为什么少吃还是会胖？".to_string())
            } else {
                current_pack.editable_script.hook.clone()
            };
            format!("参考 {best_label} 的开场方式，首句直接抛出“{candidate}”这类反常识问题，再用下一句补解释，不要先铺背景。")
        }
        "authority" => {
            let persona = preview_text(&current_pack.speaker_profile.persona, "专业健康讲师", 22);
            format!("参考 {best_label} 的首屏身份表达，在信息条提前补出“{persona} / 专业研究背景 / 服务对象”这类可信线索。")
        }
        "subtitles" => {
            let keyword = current_pack
                .cover_copy_candidates
                .first()
                .cloned()
                .or_else(|| current_pack.title_candidates.first().cloned())
                .unwrap_or_else(|| "压力不降，少吃也胖".to_string());
            format!("参考 {best_label} 的字幕压缩方式，把字幕控制到每屏 10-14 个字，并优先亮出“{keyword}”这类关键词。")
        }
        "rhythm" => {
            let hook = preview_text(&current_pack.editable_script.hook, "少吃不一定会瘦", 18);
            format!("参考 {best_label} 的节奏，围绕“{hook}”按“结论 -> 原因 -> 机制 -> 落点”重排结构，每句只保留一个信息点。")
        }
        "background" => {
            let visual = preview_text(
                &current_pack.video_prompt_draft.visual_brief,
                "书房背景",
                18,
            );
            format!("参考 {best_label} 的空间层次，保留“{visual}”这类线索，再补进书架、窗帘、桌面道具和景深层次。")
        }
        "lighting" => {
            let tone = preview_text(&current_pack.speaker_profile.tone, "专业可信", 16);
            format!("参考 {best_label} 的布光处理，保持“{tone}”的人物状态，同时明确“柔和主光 + 轻微侧补光 + 自然肤色 + 暖中性色调”。")
        }
        "framing" => {
            let framing = preview_text(
                &current_pack.video_prompt_draft.reusable_prompt,
                "竖屏中近景",
                18,
            );
            format!("参考 {best_label} 的镜头组织，把机位约束成“{framing}”这一类稳定构图，并给字幕预留安全区。")
        }
        _ => {
            let persona = if current_pack.speaker_profile.persona.trim().is_empty() {
                "专业健康讲师".to_string()
            } else {
                current_pack.speaker_profile.persona.clone()
            };
            format!("参考 {best_label} 的口播状态，把人物表达统一成“{persona}”这一路数，语速更稳，停顿更自然，减少念稿感。")
        }
    }
}

fn build_competitor_metric_report(
    key: &str,
    current_pack: &MaterialPackFile,
    competitors: &[(CompetitorManifestSample, MaterialPackFile)],
) -> CompetitorMetricReport {
    let (label, summary, action, prompt_focus, tweak_slice) = competitor_metric_meta(key);
    let current_score = score_competitor_metric(key, current_pack);
    let current_note = competitor_metric_note(key, current_pack);

    let mut scored = competitors
        .iter()
        .map(|(sample, pack)| {
            (
                sample.label.clone(),
                score_competitor_metric(key, pack),
                competitor_metric_note(key, pack),
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let competitor_scores = scored
        .iter()
        .map(|(_, score, _)| *score)
        .collect::<Vec<_>>();
    let competitor_score = average_f64(&competitor_scores);
    let (best_label, competitor_best_score, best_note) =
        scored.first().cloned().unwrap_or_else(|| {
            (
                "竞品样本".to_string(),
                current_score,
                "没有可用竞品说明。".to_string(),
            )
        });
    let evidence = scored
        .iter()
        .take(3)
        .map(|(label, score, note)| format!("{label} · {score:.1} 分 · {note}"))
        .collect::<Vec<_>>();

    CompetitorMetricReport {
        key: key.to_string(),
        label: label.to_string(),
        summary: summary.to_string(),
        current_score,
        competitor_score,
        competitor_best_score,
        current_note,
        benchmark_note: format!(
            "竞品均值 {:.1}；当前最佳样本 {}：{}",
            competitor_score, best_label, best_note
        ),
        action: action.to_string(),
        rewrite_hint: rewrite_hint_for_metric(key, current_pack, &best_label),
        prompt_tweaks: tweak_slice
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        prompt_focus: prompt_focus.to_string(),
        evidence,
    }
}

fn load_settings_envelope() -> Result<RuntimeSettingsEnvelope, String> {
    let path = settings_path()?;
    if !path.exists() {
        let defaults = default_settings_envelope();
        save_settings_envelope(&defaults)?;
        return Ok(defaults);
    }
    let raw = read_json_file(&path)?;
    if let Ok(mut envelope) = serde_json::from_str::<RuntimeSettingsEnvelope>(&raw) {
        envelope.settings = hydrate_settings_from_external(envelope.settings);
        save_settings_envelope(&envelope)?;
        return Ok(envelope);
    }
    let legacy = serde_json::from_str::<RuntimeSettingsFile>(&raw)
        .map_err(|e| format!("failed to parse settings at {}: {e}", path.display()))?;
    let upgraded = RuntimeSettingsEnvelope {
        schema_version: SETTINGS_SCHEMA_VERSION,
        updated_at_ms: now_ms(),
        settings: hydrate_settings_from_external(legacy),
    };
    save_settings_envelope(&upgraded)?;
    Ok(upgraded)
}

fn save_settings_envelope(envelope: &RuntimeSettingsEnvelope) -> Result<(), String> {
    let path = settings_path()?;
    write_json_file(&path, envelope)
}

fn to_settings_view(envelope: &RuntimeSettingsEnvelope) -> Result<RuntimeSettingsView, String> {
    let path = settings_path()?;
    Ok(RuntimeSettingsView {
        schema_version: envelope.schema_version,
        updated_at_ms: envelope.updated_at_ms,
        settings_path: path.display().to_string(),
        text_provider: TextProviderView {
            default_tier: envelope.settings.text_provider.default_tier.clone(),
            route_kind: envelope.settings.text_provider.route_kind.clone(),
            has_api_key: !envelope.settings.text_provider.api_key.trim().is_empty(),
            api_key_masked: mask_secret(&envelope.settings.text_provider.api_key),
            custom_base_url: envelope.settings.text_provider.custom_base_url.clone(),
            presets: envelope.settings.text_provider.presets.clone(),
        },
        vision_provider: VisionProviderView {
            has_api_key: !envelope.settings.vision_provider.api_key.trim().is_empty(),
            api_key_masked: mask_secret(&envelope.settings.vision_provider.api_key),
            model: envelope.settings.vision_provider.model.clone(),
            base_url: envelope.settings.vision_provider.base_url.clone(),
            allow_advanced_override: envelope.settings.vision_provider.allow_advanced_override,
        },
        budget: envelope.settings.budget.clone(),
        limits: envelope.settings.limits.clone(),
    })
}

fn normalize_tier(input: &str) -> &'static str {
    if input.eq_ignore_ascii_case("pro") {
        "pro"
    } else {
        "flash"
    }
}

fn normalize_mode(input: &str) -> &'static str {
    match input {
        "review" => "review",
        "competitor" => "competitor",
        _ => "extract",
    }
}

fn normalize_source_kind(input: &str) -> &'static str {
    if input == "local_video" {
        "local_video"
    } else {
        "douyin_url"
    }
}

fn effective_text_preset<'a>(settings: &'a RuntimeSettingsFile, tier: &str) -> &'a TextPreset {
    if normalize_tier(tier) == "pro" {
        &settings.text_provider.presets.pro
    } else {
        &settings.text_provider.presets.flash
    }
}

fn effective_text_base_url(settings: &RuntimeSettingsFile, tier: &str) -> String {
    let preset = effective_text_preset(settings, tier);
    if settings.text_provider.route_kind == "custom"
        && !settings.text_provider.custom_base_url.trim().is_empty()
    {
        settings.text_provider.custom_base_url.clone()
    } else {
        preset.base_url.clone()
    }
}

fn text_usage_cost_cny(settings: &RuntimeSettingsFile, tier: &str, usage: &LlmUsage) -> f64 {
    let (input_rate, output_rate) = if normalize_tier(tier) == "pro" {
        (
            settings.budget.pro_input_per_m_tokens_cny,
            settings.budget.pro_output_per_m_tokens_cny,
        )
    } else {
        (
            settings.budget.flash_input_per_m_tokens_cny,
            settings.budget.flash_output_per_m_tokens_cny,
        )
    };

    let prompt_cost = (usage.prompt_tokens as f64 / 1_000_000.0) * input_rate;
    let completion_cost = (usage.completion_tokens as f64 / 1_000_000.0) * output_rate;
    round4(prompt_cost + completion_cost)
}

fn apply_settings_update(
    current: RuntimeSettingsEnvelope,
    update: RuntimeSettingsUpdate,
) -> RuntimeSettingsEnvelope {
    RuntimeSettingsEnvelope {
        schema_version: SETTINGS_SCHEMA_VERSION,
        updated_at_ms: now_ms(),
        settings: RuntimeSettingsFile {
            text_provider: TextProviderFile {
                default_tier: normalize_tier(&update.text_provider.default_tier).to_string(),
                route_kind: if update.text_provider.route_kind == "custom" {
                    "custom".to_string()
                } else {
                    "official".to_string()
                },
                api_key: if update.text_provider.text_api_key.trim().is_empty() {
                    current.settings.text_provider.api_key
                } else {
                    update.text_provider.text_api_key.trim().to_string()
                },
                custom_base_url: update.text_provider.custom_base_url.trim().to_string(),
                presets: TextPresetsFile {
                    flash: TextPreset {
                        model: if update.text_provider.presets.flash.model.trim().is_empty() {
                            DEFAULT_FLASH_MODEL.to_string()
                        } else {
                            update.text_provider.presets.flash.model.trim().to_string()
                        },
                        base_url: DEFAULT_DEEPSEEK_URL.to_string(),
                    },
                    pro: TextPreset {
                        model: if update.text_provider.presets.pro.model.trim().is_empty() {
                            DEFAULT_PRO_MODEL.to_string()
                        } else {
                            update.text_provider.presets.pro.model.trim().to_string()
                        },
                        base_url: DEFAULT_DEEPSEEK_URL.to_string(),
                    },
                },
            },
            vision_provider: VisionProviderFile {
                api_key: if update.vision_provider.vision_api_key.trim().is_empty() {
                    current.settings.vision_provider.api_key
                } else {
                    update.vision_provider.vision_api_key.trim().to_string()
                },
                model: if update.vision_provider.allow_advanced_override
                    && !update.vision_provider.model.trim().is_empty()
                {
                    update.vision_provider.model.trim().to_string()
                } else {
                    DEFAULT_VISION_MODEL.to_string()
                },
                base_url: if update.vision_provider.allow_advanced_override
                    && !update.vision_provider.base_url.trim().is_empty()
                {
                    update.vision_provider.base_url.trim().to_string()
                } else {
                    DEFAULT_VISION_URL.to_string()
                },
                allow_advanced_override: update.vision_provider.allow_advanced_override,
            },
            budget: update.budget,
            limits: LimitsFile {
                max_frames: update.limits.max_frames.max(1),
                max_competitors: update.limits.max_competitors.max(1),
                max_transcription_minutes: update.limits.max_transcription_minutes.max(1),
                auto_ocr: update.limits.auto_ocr,
                auto_asr: update.limits.auto_asr,
            },
        },
    }
}

fn estimate_job_cost_impl(
    settings: &RuntimeSettingsFile,
    request: &EstimateJobRequest,
) -> EstimateJobResult {
    let effective_frames = request.frame_count.min(settings.limits.max_frames).max(1);
    let effective_competitors = request
        .competitor_count
        .min(settings.limits.max_competitors);
    let effective_minutes = request
        .duration_minutes
        .min(settings.limits.max_transcription_minutes)
        .max(1);
    let mode = normalize_mode(&request.mode);

    let base_prompt = match mode {
        "review" => 2100,
        "competitor" => 2600,
        _ => 1800,
    };
    let base_completion = match mode {
        "review" => 950,
        "competitor" => 1400,
        _ => 1250,
    };

    let transcript_prompt = effective_minutes * 480;
    let frame_prompt = effective_frames * 140;
    let competitor_prompt = if mode == "competitor" {
        effective_competitors * 650
    } else {
        0
    };
    let prompt_tokens = base_prompt + transcript_prompt + frame_prompt + competitor_prompt;

    let frame_completion = effective_frames * 45;
    let competitor_completion = if mode == "competitor" {
        effective_competitors * 220
    } else {
        0
    };
    let completion_tokens = base_completion + frame_completion + competitor_completion;

    let vl_calls = ((effective_frames + 3) / 4).max(1);
    let text_calls = match mode {
        "review" => 2,
        "competitor" => 4,
        _ => 3,
    };

    let tier = normalize_tier(&request.text_tier);
    let (input_rate, output_rate) = if tier == "pro" {
        (
            settings.budget.pro_input_per_m_tokens_cny,
            settings.budget.pro_output_per_m_tokens_cny,
        )
    } else {
        (
            settings.budget.flash_input_per_m_tokens_cny,
            settings.budget.flash_output_per_m_tokens_cny,
        )
    };

    let text_cost = (prompt_tokens as f64 / 1_000_000.0) * input_rate
        + (completion_tokens as f64 / 1_000_000.0) * output_rate;
    let vl_cost = effective_frames as f64
        * (settings.budget.vl_input_per_frame_cny + settings.budget.vl_output_per_frame_cny);
    let total = text_cost + vl_cost;

    let mut notes = Vec::new();
    if request.frame_count > effective_frames {
        notes.push(format!("抽帧数已按上限收敛到 {} 帧。", effective_frames));
    }
    if request.competitor_count > effective_competitors {
        notes.push(format!(
            "竞品数量已按上限收敛到 {} 条。",
            effective_competitors
        ));
    }
    if request.duration_minutes > effective_minutes {
        notes.push(format!(
            "转写时长已按上限收敛到 {} 分钟。",
            effective_minutes
        ));
    }
    if normalize_source_kind(&request.source_kind) == "douyin_url" {
        notes.push("抖音链路后续还会额外增加下载与预处理时间。".to_string());
    }
    if mode == "competitor" && effective_competitors == 0 {
        notes.push("竞品分析模式建议至少提供 1 条竞品。".to_string());
    }

    EstimateJobResult {
        estimated_prompt_tokens: prompt_tokens,
        estimated_completion_tokens: completion_tokens,
        estimated_vl_frames: effective_frames,
        estimated_vl_calls: vl_calls,
        estimated_text_calls: text_calls,
        estimated_cost_cny: round2(total),
        exceeds_job_budget: total > settings.budget.per_job_cny,
        effective_text_model: effective_text_preset(settings, tier).model.clone(),
        effective_text_base_url: effective_text_base_url(settings, tier),
        effective_vision_model: settings.vision_provider.model.clone(),
        notes,
    }
}

fn run_extract_ingest_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    ensure_job_layout(&item.artifact_dir)?;
    match item.source_kind.as_str() {
        "local_video" => ingest_local_video(&item.source_value, &item.artifact_dir),
        _ => ingest_douyin_video(&item.source_value, &item.artifact_dir),
    }
}

fn run_extract_preprocess_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    ensure_job_layout(&item.artifact_dir)?;
    let source_video = source_video_path(&item.artifact_dir)?;
    let report =
        validate_video_file_l0(&source_video, None, 0.0, false).map_err(|e| e.to_string())?;
    write_json_file(
        &job_derived_dir(&item.artifact_dir).join("media_probe.json"),
        &report.layers_json(),
    )?;
    if !report.passed {
        return Err(report.reasons.join("；"));
    }

    let frames_dir = job_frames_dir(&item.artifact_dir);
    if frames_dir.exists() {
        for entry in fs::read_dir(&frames_dir)
            .map_err(|e| format!("read frames dir {} failed: {e}", frames_dir.display()))?
        {
            let entry = entry.map_err(|e| format!("read frame entry failed: {e}"))?;
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
    }

    let frame_count = item.frame_count.max(1) as usize;
    let frames =
        extract_keyframes_scaled(&source_video, frame_count, 768).map_err(|e| e.to_string())?;
    let mut frame_manifest = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let path = frames_dir.join(format!("frame_{:04}.jpg", index + 1));
        fs::write(&path, frame)
            .map_err(|e| format!("write frame {} failed: {e}", path.display()))?;
        frame_manifest.push(json!({
            "frame_id": format!("frame_{:04}", index + 1),
            "order": index + 1,
            "path": path_string(&path),
        }));
    }
    write_json_file(
        &job_derived_dir(&item.artifact_dir).join("keyframes.json"),
        &json!({ "frames": frame_manifest }),
    )?;

    let mut notes = vec![format!("已生成 {} 张抽样帧。", frames.len())];
    if report
        .probe
        .as_ref()
        .map(|probe| probe.audio_streams > 0)
        .unwrap_or(false)
    {
        let audio_path = job_derived_dir(&item.artifact_dir).join("audio.wav");
        extract_audio_wav(&source_video, &audio_path)?;
        notes.push(format!("音频已提取到 {}", audio_path.display()));
    } else {
        notes.push("源视频不含音频流，跳过 audio.wav 提取。".to_string());
    }
    Ok(notes)
}

fn run_extract_ocr_stage(
    item: &JobWorkItem,
    settings: &RuntimeSettingsFile,
) -> Result<Vec<String>, String> {
    let ocr_path = job_derived_dir(&item.artifact_dir).join("ocr.json");
    if !settings.limits.auto_ocr {
        write_json_file(
            &ocr_path,
            &json!({
                "provider": "disabled",
                "status": "skipped",
                "text": "none detected",
                "lines": [],
            }),
        )?;
        return Ok(vec!["自动 OCR 已关闭，本阶段已跳过。".to_string()]);
    }

    let source_video = source_video_path(&item.artifact_dir)?;
    let endpoint = build_vl_endpoint(settings)?;
    let ocr_frames = extract_keyframes_scaled(
        &source_video,
        item.frame_count.clamp(1, 6) as usize,
        OCR_MAX_WIDTH,
    )
    .map_err(|e| e.to_string())?;
    let payload: Vec<String> = ocr_frames.iter().map(|frame| encode_frame(frame)).collect();
    let subtitle_ocr = request_subtitle_ocr(&endpoint, &payload).map_err(|e| e.to_string())?;
    let lines = split_text_lines(&subtitle_ocr);
    let line_items = lines
        .iter()
        .enumerate()
        .map(|(index, text)| {
            json!({
                "id": format!("ocr_{:03}", index + 1),
                "order": index + 1,
                "text": text,
            })
        })
        .collect::<Vec<_>>();
    write_json_file(
        &ocr_path,
        &json!({
            "provider": settings.vision_provider.model,
            "status": "completed",
            "text": subtitle_ocr,
            "lines": line_items,
        }),
    )?;
    Ok(vec![format!("OCR 已提取 {} 行字幕文本。", lines.len())])
}

fn run_extract_asr_stage(
    item: &JobWorkItem,
    settings: &RuntimeSettingsFile,
) -> Result<Vec<String>, String> {
    let asr_path = job_derived_dir(&item.artifact_dir).join("asr.json");
    let audio_path = job_derived_dir(&item.artifact_dir).join("audio.wav");
    if !settings.limits.auto_asr || !audio_path.is_file() {
        write_json_file(
            &asr_path,
            &json!({
                "provider": "disabled",
                "status": "skipped",
                "transcript": "none available",
                "segments": [],
            }),
        )?;
        return Ok(vec!["ASR 已跳过。".to_string()]);
    }

    let endpoint = build_asr_endpoint(settings)?;
    let transcript = request_transcription(&endpoint, &audio_path).map_err(|e| e.to_string())?;
    let media_probe =
        read_json_value(&job_derived_dir(&item.artifact_dir).join("media_probe.json"))?;
    let duration = media_probe
        .get("probe")
        .and_then(|probe| probe.get("duration_s"))
        .and_then(Value::as_f64)
        .unwrap_or(item.duration_minutes as f64 * 60.0);
    write_json_file(
        &asr_path,
        &json!({
            "provider": endpoint.model,
            "status": "completed",
            "transcript": transcript,
            "segments": [
                {
                    "id": "asr_001",
                    "start_sec": 0.0,
                    "end_sec": duration,
                    "speaker": "host",
                    "text": transcript,
                    "confidence": 0.9
                }
            ],
        }),
    )?;
    Ok(vec!["音频转写已完成。".to_string()])
}

fn run_extract_vision_stage(
    item: &JobWorkItem,
    settings: &RuntimeSettingsFile,
) -> Result<Vec<String>, String> {
    let source_video = source_video_path(&item.artifact_dir)?;
    let endpoint = build_vl_endpoint(settings)?;
    let frames = extract_keyframes(&source_video, item.frame_count.max(1) as usize)
        .map_err(|e| e.to_string())?;
    let payload: Vec<String> = frames.iter().map(|frame| encode_frame(frame)).collect();
    let asr_json = read_json_value(&job_derived_dir(&item.artifact_dir).join("asr.json")).ok();
    let transcript = asr_json
        .as_ref()
        .and_then(|value| value.get("transcript"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "none available")
        .map(ToOwned::to_owned);
    let artifact = request_reverse_prompt(&endpoint, &payload, transcript.as_deref())
        .map_err(|e| e.to_string())?;
    let analysis_dir = job_analysis_dir(&item.artifact_dir);
    write_json_file(&analysis_dir.join("reverse_prompt_raw.json"), &artifact)?;

    let subtitle_lines = split_text_lines(&json_string_field(&artifact, "subtitle_ocr"));
    let vision_summary = json!({
        "video_style": {
            "genre": "short_video_talking_head",
            "tone": json_string_field(&artifact, "mood"),
            "setting": json_string_field(&artifact, "environment"),
            "subtitle_style": subtitle_lines.first().cloned().unwrap_or_else(|| "none detected".to_string()),
        },
        "speaker_profile": {
            "apparent_role": json_string_field(&artifact, "subject"),
            "visual_consistency": "single_clip",
        },
        "visual_findings": [
            json_string_field(&artifact, "camera"),
            json_string_field(&artifact, "lighting"),
            json_string_field(&artifact, "style"),
        ],
    });
    write_json_file(&analysis_dir.join("vision_summary.json"), &vision_summary)?;

    let keyframes = read_json_value(&job_derived_dir(&item.artifact_dir).join("keyframes.json"))
        .ok()
        .and_then(|value| value.get("frames").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let frame_analysis = keyframes
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            json!({
                "frame_id": frame.get("frame_id").cloned().unwrap_or_else(|| json!(format!("frame_{:04}", index + 1))),
                "order": index + 1,
                "path": frame.get("path").cloned().unwrap_or_else(|| json!("")),
                "summary": json_string_field(&artifact, "subject"),
                "style": {
                    "camera": json_string_field(&artifact, "camera"),
                    "lighting": json_string_field(&artifact, "lighting"),
                }
            })
        })
        .collect::<Vec<_>>();
    write_json_file(
        &analysis_dir.join("frame_analysis.json"),
        &json!(frame_analysis),
    )?;

    let visual_style_md = format!(
        "# Visual Style\n\n## Subject\n{}\n\n## Environment\n{}\n\n## Camera\n{}\n\n## Lighting\n{}\n\n## Style\n{}\n\n## Mood\n{}\n",
        json_string_field(&artifact, "subject"),
        json_string_field(&artifact, "environment"),
        json_string_field(&artifact, "camera"),
        json_string_field(&artifact, "lighting"),
        json_string_field(&artifact, "style"),
        json_string_field(&artifact, "mood"),
    );
    write_text_file(&analysis_dir.join("visual_style.md"), &visual_style_md)?;

    Ok(vec!["VL 视觉分析已完成。".to_string()])
}

fn run_extract_text_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    let analysis_dir = job_analysis_dir(&item.artifact_dir);
    let reverse_prompt = read_json_value(&analysis_dir.join("reverse_prompt_raw.json"))?;
    let transcript = json_string_field(&reverse_prompt, "audio_transcript");
    let spoken_copy = json_string_field(&reverse_prompt, "spoken_copy");
    let promo_copy = json_string_field(&reverse_prompt, "promo_copy");
    let subtitle_lines = split_text_lines(&json_string_field(&reverse_prompt, "subtitle_ocr"));
    let keywords = subtitle_lines.iter().take(5).cloned().collect::<Vec<_>>();
    let summary = if !promo_copy.is_empty() {
        promo_copy.clone()
    } else {
        first_sentence(&transcript)
    };
    let explanation = if transcript.trim().is_empty() {
        "none available".to_string()
    } else {
        first_n_sentences(&transcript, 3)
    };
    write_json_file(
        &analysis_dir.join("transcript_structured.json"),
        &json!({
            "summary": summary,
            "segments": [
                { "id": "seg_01", "function": "hook", "text": spoken_copy },
                { "id": "seg_02", "function": "explanation", "text": explanation }
            ],
            "keywords": keywords,
            "cta": Value::Null,
        }),
    )?;

    let content_summary_md = format!(
        "# Content Summary\n\n- Summary: {}\n- Hook: {}\n- Promo: {}\n- Subject: {}\n",
        if summary.is_empty() {
            "none available"
        } else {
            &summary
        },
        if spoken_copy.is_empty() {
            "none inferred"
        } else {
            &spoken_copy
        },
        if promo_copy.is_empty() {
            "none available"
        } else {
            &promo_copy
        },
        json_string_field(&reverse_prompt, "subject"),
    );
    write_text_file(
        &analysis_dir.join("content_summary.md"),
        &content_summary_md,
    )?;

    let hook_candidates = [
        spoken_copy.clone(),
        promo_copy.clone(),
        subtitle_lines.first().cloned().unwrap_or_default(),
    ]
    .into_iter()
    .filter(|candidate| !candidate.trim().is_empty())
    .collect::<Vec<_>>();
    write_text_file(
        &analysis_dir.join("hook_candidates.md"),
        &hook_candidates
            .iter()
            .map(|candidate| format!("- {candidate}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )?;

    let title_candidates = derive_title_candidates(&item.name, &promo_copy, &subtitle_lines);
    let cover_candidates = derive_cover_candidates(&promo_copy, &subtitle_lines);
    let copy_md = format!(
        "# Copy Candidates\n\n## Title Candidates\n{}\n\n## Cover Candidates\n{}\n",
        title_candidates
            .iter()
            .map(|candidate| format!("- {candidate}"))
            .collect::<Vec<_>>()
            .join("\n"),
        cover_candidates
            .iter()
            .map(|candidate| format!("- {candidate}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    write_text_file(&analysis_dir.join("copy_candidates.md"), &copy_md)?;
    Ok(vec!["文本整理与文案候选已生成。".to_string()])
}

fn run_extract_material_pack_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    let analysis_dir = job_analysis_dir(&item.artifact_dir);
    let output_dir = job_output_dir(&item.artifact_dir);
    let reverse_prompt = read_json_value(&analysis_dir.join("reverse_prompt_raw.json"))?;
    let transcript_structured = read_json_value(&analysis_dir.join("transcript_structured.json"))?;
    let subtitle_lines = split_text_lines(&json_string_field(&reverse_prompt, "subtitle_ocr"));
    let promo_copy = json_string_field(&reverse_prompt, "promo_copy");
    let spoken_copy = json_string_field(&reverse_prompt, "spoken_copy");
    let transcript = json_string_field(&reverse_prompt, "audio_transcript");
    let topic = subtitle_lines
        .first()
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| item.name.clone());
    let title_candidates = derive_title_candidates(&topic, &promo_copy, &subtitle_lines);
    let cover_candidates = derive_cover_candidates(&promo_copy, &subtitle_lines);
    let core_message = transcript_structured
        .get("keywords")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let body_block = if transcript.trim().is_empty() {
        vec![promo_copy.clone()]
    } else {
        vec![
            first_n_sentences(&transcript, 2),
            first_n_sentences(&transcript, 4),
        ]
    };
    let material_pack = json!({
        "job_id": item.id,
        "topic": topic,
        "audience": "短视频中文用户",
        "speaker_profile": {
            "persona": json_string_field(&reverse_prompt, "subject"),
            "tone": json_string_field(&reverse_prompt, "mood"),
        },
        "core_message": if core_message.is_empty() { vec![promo_copy.clone()] } else { core_message },
        "editable_script": {
            "hook": spoken_copy,
            "body": body_block,
            "ending": promo_copy,
        },
        "title_candidates": title_candidates,
        "cover_copy_candidates": cover_candidates,
        "promo_copy": [promo_copy],
        "video_prompt_draft": {
            "visual_brief": json_string_field(&reverse_prompt, "style"),
            "spoken_brief": spoken_copy,
            "reusable_prompt": json_string_field(&reverse_prompt, "prompt"),
        },
        "evidence_refs": {
            "vision_summary": "analysis/vision_summary.json",
            "transcript_structured": "analysis/transcript_structured.json",
        }
    });
    write_json_file(&output_dir.join("material_pack.json"), &material_pack)?;

    let editable_script_md = format!(
        "# Editable Script\n\n## Hook\n{}\n\n## Body\n{}\n\n## Ending\n{}\n",
        json_string_field(&material_pack["editable_script"], "hook"),
        material_pack["editable_script"]["body"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        json_string_field(&material_pack["editable_script"], "ending"),
    );
    write_text_file(&output_dir.join("editable_script.md"), &editable_script_md)?;

    let video_prompt_md = format!(
        "# Video Prompt Draft\n\n## Visual Brief\n{}\n\n## Spoken Brief\n{}\n\n## Reusable Prompt\n{}\n",
        json_string_field(&material_pack["video_prompt_draft"], "visual_brief"),
        json_string_field(&material_pack["video_prompt_draft"], "spoken_brief"),
        json_string_field(&material_pack["video_prompt_draft"], "reusable_prompt"),
    );
    write_text_file(&output_dir.join("video_prompt_draft.md"), &video_prompt_md)?;

    Ok(vec![format!("素材包已输出到 {}", output_dir.display())])
}

fn run_competitor_ingest_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    ensure_job_layout(&item.artifact_dir)?;
    let bundle = parse_competitor_bundle(&item.source_value, &item.source_kind)?;
    if bundle.competitors.is_empty() {
        return Err("竞品分析至少需要 1 条竞品样本。".to_string());
    }

    let primary = CompetitorManifestSample {
        id: "current".to_string(),
        label: if bundle.primary.label.trim().is_empty() {
            "当前视频".to_string()
        } else {
            bundle.primary.label.clone()
        },
        kind: bundle.primary.kind.clone(),
        source_value: bundle.primary.value.clone(),
        artifact_dir: path_string(&competitor_primary_root(&item.artifact_dir)),
    };
    let competitors = bundle
        .competitors
        .into_iter()
        .enumerate()
        .map(|(index, spec)| CompetitorManifestSample {
            id: format!("competitor_{:02}", index + 1),
            label: if spec.label.trim().is_empty() {
                format!("竞品 {}", index + 1)
            } else {
                spec.label
            },
            kind: spec.kind,
            source_value: spec.value,
            artifact_dir: path_string(&competitor_sample_root(&item.artifact_dir, index)),
        })
        .collect::<Vec<_>>();
    let manifest = CompetitorManifest {
        primary,
        competitors,
    };
    write_json_file(&competitor_manifest_path(&item.artifact_dir), &manifest)?;

    let mut notes = vec![format!(
        "已写入竞品任务清单：1 条当前视频 + {} 条竞品样本。",
        manifest.competitors.len()
    )];

    for sample in std::iter::once(&manifest.primary).chain(manifest.competitors.iter()) {
        append_parent_stage_log(item, format!("开始导入 {}", sample.label));
        let work = sample_work_item(item, sample, "extract_ingest");
        let sample_notes = run_extract_ingest_stage(&work)?;
        notes.push(format!("{}：{}", sample.label, sample_notes.join("；")));
    }

    Ok(notes)
}

fn run_competitor_preprocess_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    let manifest = read_competitor_manifest(&item.artifact_dir)?;
    let mut notes = Vec::new();
    for sample in std::iter::once(&manifest.primary).chain(manifest.competitors.iter()) {
        append_parent_stage_log(item, format!("开始预处理 {}", sample.label));
        let work = sample_work_item(item, sample, "extract_preprocess");
        let sample_notes = run_extract_preprocess_stage(&work)?;
        notes.push(format!("{}：{}", sample.label, sample_notes.join("；")));
    }
    Ok(notes)
}

fn run_competitor_vision_stage(
    item: &JobWorkItem,
    settings: &RuntimeSettingsFile,
) -> Result<Vec<String>, String> {
    let manifest = read_competitor_manifest(&item.artifact_dir)?;
    let mut notes = Vec::new();
    for sample in std::iter::once(&manifest.primary).chain(manifest.competitors.iter()) {
        append_parent_stage_log(item, format!("开始 OCR / ASR / VL {}", sample.label));
        let work = sample_work_item(item, sample, "extract_vision");
        let mut sample_notes = Vec::new();
        sample_notes.extend(run_extract_ocr_stage(&work, settings)?);
        sample_notes.extend(run_extract_asr_stage(&work, settings)?);
        sample_notes.extend(run_extract_vision_stage(&work, settings)?);
        notes.push(format!("{}：{}", sample.label, sample_notes.join("；")));
    }
    Ok(notes)
}

fn run_competitor_text_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    let manifest = read_competitor_manifest(&item.artifact_dir)?;
    let mut notes = Vec::new();
    for sample in std::iter::once(&manifest.primary).chain(manifest.competitors.iter()) {
        append_parent_stage_log(item, format!("开始结构化整理 {}", sample.label));
        let work = sample_work_item(item, sample, "extract_text");
        let mut sample_notes = Vec::new();
        sample_notes.extend(run_extract_text_stage(&work)?);
        sample_notes.extend(run_extract_material_pack_stage(&work)?);
        notes.push(format!("{}：{}", sample.label, sample_notes.join("；")));
    }

    let primary_root = PathBuf::from(&manifest.primary.artifact_dir);
    copy_with_overwrite(
        &material_pack_path(&primary_root),
        &material_pack_path(&item.artifact_dir),
    )?;
    copy_if_exists(
        &job_output_dir(&primary_root).join("editable_script.md"),
        &job_output_dir(&item.artifact_dir).join("editable_script.md"),
    )?;
    copy_if_exists(
        &job_output_dir(&primary_root).join("video_prompt_draft.md"),
        &job_output_dir(&item.artifact_dir).join("video_prompt_draft.md"),
    )?;
    notes.push("已把当前视频的素材包同步到任务根输出目录。".to_string());
    Ok(notes)
}

fn build_heuristic_competitor_report(
    item: &JobWorkItem,
    manifest: &CompetitorManifest,
) -> Result<CompetitorReport, String> {
    let current_root = PathBuf::from(&manifest.primary.artifact_dir);
    let current_pack = read_material_pack_file(&material_pack_path(&current_root))?;
    let mut competitor_packs = Vec::new();
    for sample in &manifest.competitors {
        let pack = read_material_pack_file(&material_pack_path(Path::new(&sample.artifact_dir)))?;
        competitor_packs.push((sample.clone(), pack));
    }
    if competitor_packs.is_empty() {
        return Err("竞品报告生成失败：没有可用的竞品素材包。".to_string());
    }

    let metric_keys = [
        "hook",
        "authority",
        "subtitles",
        "rhythm",
        "background",
        "lighting",
        "framing",
        "persona",
    ];
    let mut metrics = metric_keys
        .iter()
        .map(|key| build_competitor_metric_report(key, &current_pack, &competitor_packs))
        .collect::<Vec<_>>();
    metrics.sort_by(|a, b| {
        let gap_a = a.competitor_score - a.current_score;
        let gap_b = b.competitor_score - b.current_score;
        gap_b
            .partial_cmp(&gap_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_findings = metrics
        .iter()
        .take(3)
        .map(|metric| {
            format!(
                "{}：当前 {:.1}，竞品基准 {:.1}，建议 {}",
                metric.label, metric.current_score, metric.competitor_score, metric.action
            )
        })
        .collect::<Vec<_>>();

    let recommended_focus = metrics
        .first()
        .map(|metric| metric.prompt_focus.clone())
        .unwrap_or_else(|| "balanced".to_string());
    let recommended_tweaks = unique_non_empty(
        metrics
            .iter()
            .take(4)
            .flat_map(|metric| metric.prompt_tweaks.clone())
            .collect::<Vec<_>>(),
    );

    Ok(CompetitorReport {
        job_id: item.id.clone(),
        current_label: manifest.primary.label.clone(),
        current_topic: if current_pack.topic.trim().is_empty() {
            item.name.clone()
        } else {
            current_pack.topic.clone()
        },
        competitor_count: competitor_packs.len(),
        competitor_labels: manifest
            .competitors
            .iter()
            .map(|sample| sample.label.clone())
            .collect::<Vec<_>>(),
        top_findings,
        recommended_focus,
        recommended_tweaks,
        metrics,
        generated_by_model: "heuristic_fallback".to_string(),
        llm_usage: LlmUsage::default(),
        generated_at_ms: now_ms(),
    })
}

fn build_competitor_report_llm_payload(
    heuristic: &CompetitorReport,
    current_pack: &MaterialPackFile,
    competitors: &[(CompetitorManifestSample, MaterialPackFile)],
) -> Value {
    json!({
        "task": "生成短视频竞品分析报告 JSON。必须输出 JSON，且字段完整。",
        "schema_hint": {
            "top_findings": ["string"],
            "recommended_focus": "balanced | lip_sync | visual",
            "recommended_tweaks": ["vivid_background | stronger_authority | clearer_hook | safer_subtitles | softer_lighting"],
            "metrics": [
                {
                    "key": "hook | authority | subtitles | rhythm | background | lighting | framing | persona",
                    "current_note": "string",
                    "benchmark_note": "string",
                    "action": "string",
                    "rewrite_hint": "string"
                }
            ]
        },
        "rules": [
            "保留原有 metrics 的 key、label、summary、current_score、competitor_score、competitor_best_score、prompt_focus、prompt_tweaks、evidence，不要增删 key。",
            "你可以优化 top_findings、recommended_focus、recommended_tweaks，以及每个 metric 的 current_note、benchmark_note、action、rewrite_hint。",
            "top_findings 控制在 3 条以内，每条都要像给运营或剪辑的执行建议。",
            "recommended_focus 只能是 balanced、lip_sync、visual 之一。",
            "recommended_tweaks 只能从给定枚举里选择。",
            "不要输出 markdown，不要输出额外解释。"
        ],
        "current_video": {
            "label": heuristic.current_label,
            "topic": heuristic.current_topic,
            "speaker_profile": current_pack.speaker_profile,
            "core_message": current_pack.core_message,
            "editable_script": current_pack.editable_script,
            "title_candidates": current_pack.title_candidates,
            "cover_copy_candidates": current_pack.cover_copy_candidates,
            "promo_copy": current_pack.promo_copy,
            "video_prompt_draft": current_pack.video_prompt_draft
        },
        "competitors": competitors.iter().map(|(sample, pack)| {
            json!({
                "label": sample.label,
                "topic": pack.topic,
                "speaker_profile": pack.speaker_profile,
                "title_candidates": pack.title_candidates,
                "cover_copy_candidates": pack.cover_copy_candidates,
                "promo_copy": pack.promo_copy,
                "video_prompt_draft": pack.video_prompt_draft
            })
        }).collect::<Vec<_>>(),
        "heuristic_report": heuristic
    })
}

fn merge_competitor_report_from_llm(
    heuristic: &CompetitorReport,
    llm_json: Value,
    model_name: &str,
    usage: LlmUsage,
) -> Result<CompetitorReport, String> {
    let top_findings = llm_json
        .get("top_findings")
        .and_then(Value::as_array)
        .map(|values| {
            unique_non_empty(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| heuristic.top_findings.clone());

    let recommended_focus = llm_json
        .get("recommended_focus")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "balanced" | "lip_sync" | "visual"))
        .unwrap_or(&heuristic.recommended_focus)
        .to_string();

    let recommended_tweaks = llm_json
        .get("recommended_tweaks")
        .and_then(Value::as_array)
        .map(|values| {
            unique_non_empty(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| heuristic.recommended_tweaks.clone());

    let llm_metric_map = llm_json
        .get("metrics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|metric| {
            let key = metric
                .get("key")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)?;
            Some((key, metric))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let metrics = heuristic
        .metrics
        .iter()
        .map(|metric| {
            let llm_metric = llm_metric_map.get(&metric.key);
            CompetitorMetricReport {
                key: metric.key.clone(),
                label: metric.label.clone(),
                summary: metric.summary.clone(),
                current_score: metric.current_score,
                competitor_score: metric.competitor_score,
                competitor_best_score: metric.competitor_best_score,
                current_note: llm_metric
                    .and_then(|value| value.get("current_note"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&metric.current_note)
                    .to_string(),
                benchmark_note: llm_metric
                    .and_then(|value| value.get("benchmark_note"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&metric.benchmark_note)
                    .to_string(),
                action: llm_metric
                    .and_then(|value| value.get("action"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&metric.action)
                    .to_string(),
                rewrite_hint: llm_metric
                    .and_then(|value| value.get("rewrite_hint"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&metric.rewrite_hint)
                    .to_string(),
                prompt_tweaks: metric.prompt_tweaks.clone(),
                prompt_focus: metric.prompt_focus.clone(),
                evidence: metric.evidence.clone(),
            }
        })
        .collect::<Vec<_>>();

    Ok(CompetitorReport {
        job_id: heuristic.job_id.clone(),
        current_label: heuristic.current_label.clone(),
        current_topic: heuristic.current_topic.clone(),
        competitor_count: heuristic.competitor_count,
        competitor_labels: heuristic.competitor_labels.clone(),
        top_findings,
        recommended_focus,
        recommended_tweaks,
        metrics,
        generated_by_model: model_name.to_string(),
        llm_usage: usage,
        generated_at_ms: now_ms(),
    })
}

fn build_competitor_report(
    item: &JobWorkItem,
    manifest: &CompetitorManifest,
) -> Result<CompetitorReport, String> {
    let heuristic = build_heuristic_competitor_report(item, manifest)?;
    let current_root = PathBuf::from(&manifest.primary.artifact_dir);
    let current_pack = read_material_pack_file(&material_pack_path(&current_root))?;
    let mut competitor_packs = Vec::new();
    for sample in &manifest.competitors {
        let pack = read_material_pack_file(&material_pack_path(Path::new(&sample.artifact_dir)))?;
        competitor_packs.push((sample.clone(), pack));
    }
    let settings = load_settings_envelope()?.settings;
    let endpoint = build_text_endpoint(&settings, item)?;
    let payload = build_competitor_report_llm_payload(&heuristic, &current_pack, &competitor_packs);
    let (llm_json, usage) = request_structured_competitor_report(&endpoint, &payload)?;
    merge_competitor_report_from_llm(&heuristic, llm_json, &endpoint.model, usage)
}

fn run_competitor_report_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    let manifest = read_competitor_manifest(&item.artifact_dir)?;
    let (report, fallback_note) = match build_competitor_report(item, &manifest) {
        Ok(report) => (report, None),
        Err(err) => {
            let heuristic = build_heuristic_competitor_report(item, &manifest)?;
            (heuristic, Some(err))
        }
    };
    write_json_file(&competitor_report_path(&item.artifact_dir), &report)?;

    let md = format!(
        "# Competitor Report\n\n- Current: {}\n- Topic: {}\n- Competitors: {}\n- Generated By: {}\n- LLM Usage: prompt {} / completion {} / total {}\n- Recommended Focus: {}\n- Recommended Tweaks: {}\n\n## Top Findings\n{}\n\n## Metrics\n{}\n",
        report.current_label,
        report.current_topic,
        report.competitor_labels.join(" / "),
        report.generated_by_model,
        report.llm_usage.prompt_tokens,
        report.llm_usage.completion_tokens,
        report.llm_usage.total_tokens,
        report.recommended_focus,
        if report.recommended_tweaks.is_empty() {
            "none".to_string()
        } else {
            report.recommended_tweaks.join(" / ")
        },
        report
            .top_findings
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        report
            .metrics
            .iter()
            .map(|metric| {
                format!(
                    "### {}\n- Current: {:.1}\n- Competitor Baseline: {:.1}\n- Action: {}\n- Rewrite Hint: {}\n- Evidence:\n{}\n",
                    metric.label,
                    metric.current_score,
                    metric.competitor_score,
                    metric.action,
                    metric.rewrite_hint,
                    metric
                        .evidence
                        .iter()
                        .map(|line| format!("  - {line}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
    write_text_file(
        &job_output_dir(&item.artifact_dir).join("competitor_report.md"),
        &md,
    )?;

    Ok(vec![
        format!(
            "真实竞品报告已生成：{} 条竞品样本已完成对比。",
            report.competitor_count
        ),
        format!("报告生成模型：{}", report.generated_by_model),
        if report.llm_usage.total_tokens > 0 {
            format!(
                "本次报告实际用量：prompt {} / completion {} / total {} tokens。",
                report.llm_usage.prompt_tokens,
                report.llm_usage.completion_tokens,
                report.llm_usage.total_tokens
            )
        } else {
            "本次报告未记录到实际 token，用启发式报告兜底。".to_string()
        },
        fallback_note
            .map(|err| format!("DeepSeek 调用失败，已回退启发式报告：{err}"))
            .unwrap_or_else(|| "DeepSeek 结构化竞品报告已成功生成。".to_string()),
        format!("推荐优化目标：{}", report.recommended_focus),
    ])
}

fn process_extract_stage(
    item: &JobWorkItem,
    settings: &RuntimeSettingsFile,
) -> Result<Vec<String>, String> {
    match item.stage_key.as_str() {
        "extract_ingest" => run_extract_ingest_stage(item),
        "extract_preprocess" => run_extract_preprocess_stage(item),
        "extract_ocr" => run_extract_ocr_stage(item, settings),
        "extract_asr" => run_extract_asr_stage(item, settings),
        "extract_vision" => run_extract_vision_stage(item, settings),
        "extract_text" => run_extract_text_stage(item),
        "extract_material_pack" => run_extract_material_pack_stage(item),
        other => Err(format!("unsupported extract stage {other}")),
    }
}

fn process_competitor_stage(
    item: &JobWorkItem,
    settings: &RuntimeSettingsFile,
) -> Result<Vec<String>, String> {
    match item.stage_key.as_str() {
        "competitor_ingest" => run_competitor_ingest_stage(item),
        "competitor_preprocess" => run_competitor_preprocess_stage(item),
        "competitor_vision" => run_competitor_vision_stage(item, settings),
        "competitor_text" => run_competitor_text_stage(item),
        "competitor_report" => run_competitor_report_stage(item),
        other => Err(format!("unsupported competitor stage {other}")),
    }
}

fn process_job_stage(item: &JobWorkItem) -> Result<Vec<String>, String> {
    let settings = load_settings_envelope()?.settings;
    match item.mode.as_str() {
        "extract" => process_extract_stage(item, &settings),
        "competitor" => process_competitor_stage(item, &settings),
        "review" => {
            Err("当前版本只接通了 extract / competitor 链路，review 还未挂真实执行器。".to_string())
        }
        other => Err(format!("unsupported job mode {other}")),
    }
}

impl JobStore {
    fn load_or_create() -> Result<Self, String> {
        let file_path = jobs_file_path()?;
        let jobs_root = jobs_root_dir()?;
        fs::create_dir_all(&jobs_root).map_err(|e| e.to_string())?;

        if !file_path.exists() {
            let store = Self {
                file_path,
                jobs_root,
                queue: JobQueueFile {
                    schema_version: JOBS_SCHEMA_VERSION,
                    updated_at_ms: now_ms(),
                    next_seq: 1,
                    jobs: Vec::new(),
                },
            };
            store.save()?;
            return Ok(store);
        }

        let raw = read_json_file(&file_path)?;
        let queue = serde_json::from_str::<JobQueueFile>(&raw)
            .map_err(|e| format!("failed to parse jobs at {}: {e}", file_path.display()))?;
        Ok(Self {
            file_path,
            jobs_root,
            queue,
        })
    }

    fn save(&self) -> Result<(), String> {
        write_json_file(&self.file_path, &self.queue)
    }

    fn list_jobs(&self) -> Vec<JobView> {
        let mut jobs: Vec<JobView> = self.queue.jobs.iter().map(JobView::from).collect();
        jobs.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        jobs
    }

    fn dashboard(&self) -> DashboardSnapshot {
        let now = now_ms();
        let lookback = 24 * 60 * 60 * 1000;
        let pending_jobs = self
            .queue
            .jobs
            .iter()
            .filter(|job| job.status == "waiting")
            .count() as u32;
        let running_jobs = self
            .queue
            .jobs
            .iter()
            .filter(|job| job.status == "running")
            .count() as u32;
        let finished_jobs_today = self
            .queue
            .jobs
            .iter()
            .filter(|job| job.status == "done")
            .filter(|job| job.finished_at_ms.unwrap_or(0) >= now.saturating_sub(lookback))
            .count() as u32;
        let queue_cost_today_cny: f64 = self
            .queue
            .jobs
            .iter()
            .filter(|job| job.updated_at_ms >= now.saturating_sub(lookback))
            .map(|job| {
                if job.status == "done" {
                    job.actual_cost_cny
                } else if job.status == "running" {
                    job.actual_cost_cny
                } else {
                    job.estimated_cost_cny
                }
            })
            .sum();
        let usage_cost = usage_cost_today_cny().unwrap_or(0.0);
        let estimated_spend_today_cny = round4(queue_cost_today_cny + usage_cost);
        DashboardSnapshot {
            pending_jobs,
            running_jobs,
            finished_jobs_today,
            estimated_spend_today_cny,
        }
    }

    fn active_batch_estimated_cost_cny(&self) -> f64 {
        round2(
            self.queue
                .jobs
                .iter()
                .filter(|job| matches!(job.status.as_str(), "waiting" | "running"))
                .map(|job| job.estimated_cost_cny.max(job.actual_cost_cny))
                .sum(),
        )
    }

    fn create_job(
        &mut self,
        request: CreateJobRequest,
        settings: &RuntimeSettingsFile,
    ) -> Result<JobView, String> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err("任务名称不能为空。".to_string());
        }
        let raw_source_value = request.source_value.trim();
        if raw_source_value.is_empty() {
            return Err("任务来源不能为空。".to_string());
        }
        let mode = normalize_mode(&request.mode).to_string();
        let mut source_kind = normalize_source_kind(&request.source_kind).to_string();
        let mut source_value = raw_source_value.to_string();
        let mut competitor_count = request.competitor_count;

        if mode == "competitor" {
            let mut bundle = parse_competitor_bundle(&source_value, &source_kind)?;
            if bundle.competitors.is_empty() {
                return Err("竞品分析至少需要 1 条竞品样本。".to_string());
            }
            if bundle.competitors.len() > settings.limits.max_competitors as usize {
                bundle
                    .competitors
                    .truncate(settings.limits.max_competitors as usize);
            }
            competitor_count = bundle.competitors.len() as u32;
            source_kind = bundle.primary.kind.clone();
            source_value = serde_json::to_string(&bundle)
                .map_err(|e| format!("serialize competitor bundle failed: {e}"))?;
        }

        let estimate_request = EstimateJobRequest {
            mode: mode.clone(),
            source_kind: source_kind.clone(),
            duration_minutes: request.duration_minutes.max(1),
            frame_count: request.frame_count.max(1),
            competitor_count,
            text_tier: normalize_tier(&request.text_tier).to_string(),
        };
        let estimate = estimate_job_cost_impl(settings, &estimate_request);
        if estimate.exceeds_job_budget && settings.budget.block_when_over_budget {
            return Err(format!(
                "预计成本 ¥{:.2} 超出单任务预算 ¥{:.2}，当前策略不允许启动。",
                estimate.estimated_cost_cny, settings.budget.per_job_cny
            ));
        }
        let projected_batch_cost =
            round2(self.active_batch_estimated_cost_cny() + estimate.estimated_cost_cny);
        let exceeds_batch_budget = settings.budget.per_batch_cny > 0.0
            && projected_batch_cost > settings.budget.per_batch_cny;
        if exceeds_batch_budget && settings.budget.block_when_over_budget {
            return Err(format!(
                "预计批次成本 ¥{:.2} 超出批次预算 ¥{:.2}，当前策略不允许入队。",
                projected_batch_cost, settings.budget.per_batch_cny
            ));
        }

        let id = format!("job_{}_{}", now_ms(), self.queue.next_seq);
        self.queue.next_seq += 1;
        let artifact_dir = self.jobs_root.join(&id);
        fs::create_dir_all(&artifact_dir).map_err(|e| e.to_string())?;

        let created_at_ms = now_ms();
        let target_prompt_tokens =
            ((estimate.estimated_prompt_tokens as f64) * 0.92).round() as u32;
        let target_completion_tokens =
            ((estimate.estimated_completion_tokens as f64) * 0.88).round() as u32;
        let target_cost_cny = round2((estimate.estimated_cost_cny * 0.91).max(0.02));

        let mut notes = estimate.notes.clone();
        if estimate.exceeds_job_budget && !settings.budget.block_when_over_budget {
            notes.push("该任务超过预算但已按“仅警告”策略入队。".to_string());
        }
        if exceeds_batch_budget && !settings.budget.block_when_over_budget {
            notes.push(format!(
                "加上当前任务后，批次预计成本约 ¥{projected_batch_cost:.2}，已超出批次预算但按“仅警告”策略入队。"
            ));
        }

        let record = JobRecord {
            id: id.clone(),
            name: name.to_string(),
            mode: estimate_request.mode.clone(),
            source_kind: estimate_request.source_kind.clone(),
            source_value: source_value.clone(),
            status: "waiting".to_string(),
            stage_key: "queued".to_string(),
            progress: 0,
            text_tier: estimate_request.text_tier.clone(),
            duration_minutes: estimate_request.duration_minutes,
            frame_count: estimate_request.frame_count,
            competitor_count: estimate_request.competitor_count,
            estimated_prompt_tokens: estimate.estimated_prompt_tokens,
            estimated_completion_tokens: estimate.estimated_completion_tokens,
            estimated_vl_frames: estimate.estimated_vl_frames,
            estimated_vl_calls: estimate.estimated_vl_calls,
            estimated_text_calls: estimate.estimated_text_calls,
            estimated_cost_cny: estimate.estimated_cost_cny,
            effective_text_model: estimate.effective_text_model.clone(),
            effective_text_base_url: estimate.effective_text_base_url.clone(),
            effective_vision_model: estimate.effective_vision_model.clone(),
            target_prompt_tokens,
            target_completion_tokens,
            target_cost_cny,
            actual_prompt_tokens: 0,
            actual_completion_tokens: 0,
            actual_cost_cny: 0.0,
            created_at_ms,
            updated_at_ms: created_at_ms,
            started_at_ms: None,
            finished_at_ms: None,
            artifact_dir: artifact_dir.display().to_string(),
            notes,
            error: None,
            stage_index: 0,
        };

        self.queue.jobs.push(record.clone());
        self.queue.updated_at_ms = now_ms();
        self.save()?;
        let _ = append_stage_log(&artifact_dir, "queued", "任务已创建，等待进入执行队列。");
        Ok(JobView::from(&record))
    }

    fn get_job(&self, job_id: &str) -> Option<JobView> {
        self.queue
            .jobs
            .iter()
            .find(|job| job.id == job_id)
            .map(JobView::from)
    }

    fn job_artifact_dir(&self, job_id: &str) -> Result<PathBuf, String> {
        self.queue
            .jobs
            .iter()
            .find(|job| job.id == job_id)
            .map(|job| PathBuf::from(&job.artifact_dir))
            .ok_or_else(|| format!("找不到任务 {job_id}。"))
    }

    fn job_material_pack_path(&self, job_id: &str) -> Result<PathBuf, String> {
        let artifact_dir = self.job_artifact_dir(job_id)?;
        let path = material_pack_path(&artifact_dir);
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("任务 {job_id} 的 material_pack 尚未生成。"))
        }
    }

    fn read_material_pack(&self, job_id: &str) -> Result<Value, String> {
        let path = self.job_material_pack_path(job_id)?;
        read_json_value(&path)
    }

    fn job_competitor_report_path(&self, job_id: &str) -> Result<PathBuf, String> {
        let artifact_dir = self.job_artifact_dir(job_id)?;
        let path = competitor_report_path(&artifact_dir);
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("任务 {job_id} 的 competitor_report 尚未生成。"))
        }
    }

    fn read_competitor_report(&self, job_id: &str) -> Result<Value, String> {
        let path = self.job_competitor_report_path(job_id)?;
        read_json_value(&path)
    }

    fn read_stage_log(&self, job_id: &str) -> Result<String, String> {
        let artifact_dir = self.job_artifact_dir(job_id)?;
        let path = stage_log_path(&artifact_dir);
        if !path.exists() {
            return Ok(String::new());
        }
        fs::read_to_string(&path).map_err(|e| format!("read {} failed: {e}", path.display()))
    }

    fn prepare_next_work_item(&mut self) -> Result<Option<JobWorkItem>, String> {
        if let Some(index) = self
            .queue
            .jobs
            .iter()
            .position(|job| job.status == "running")
        {
            return Ok(Some(JobWorkItem::from(&self.queue.jobs[index])));
        }

        if let Some(index) = self
            .queue
            .jobs
            .iter()
            .position(|job| job.status == "waiting")
        {
            start_waiting_job(&mut self.queue.jobs[index]);
            self.queue.updated_at_ms = now_ms();
            self.save()?;
            return Ok(Some(JobWorkItem::from(&self.queue.jobs[index])));
        }

        Ok(None)
    }

    fn finish_stage(&mut self, job_id: &str, notes: Vec<String>) -> Result<(), String> {
        let job = self
            .queue
            .jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or_else(|| format!("找不到任务 {job_id}。"))?;
        if job.status != "running" {
            return Ok(());
        }
        let stage_key = job.stage_key.clone();
        for note in notes {
            if !note.trim().is_empty() {
                job.notes.push(format!("[{}] {}", stage_key, note));
                let _ = append_stage_log(Path::new(&job.artifact_dir), &stage_key, &note);
            }
        }
        let _ = append_stage_log(
            Path::new(&job.artifact_dir),
            &stage_key,
            "阶段完成，准备推进到下一阶段。",
        );
        advance_running_job(job);
        self.queue.updated_at_ms = now_ms();
        self.save()
    }

    fn fail_job(&mut self, job_id: &str, error: String) -> Result<(), String> {
        let timestamp = now_ms();
        let job = self
            .queue
            .jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or_else(|| format!("找不到任务 {job_id}。"))?;
        if job.status != "running" {
            return Ok(());
        }
        job.status = "blocked".to_string();
        job.error = Some(error.clone());
        job.finished_at_ms = Some(timestamp);
        job.updated_at_ms = timestamp;
        job.notes
            .push(format!("[{}] 阶段执行失败：{error}", job.stage_key));
        let _ = append_stage_log(
            Path::new(&job.artifact_dir),
            &job.stage_key,
            &format!("阶段执行失败：{error}"),
        );
        self.queue.updated_at_ms = timestamp;
        self.save()
    }

    fn cancel_job(&mut self, job_id: &str) -> Result<JobView, String> {
        let timestamp = now_ms();
        let view: JobView = {
            let job = self
                .queue
                .jobs
                .iter_mut()
                .find(|job| job.id == job_id)
                .ok_or_else(|| format!("找不到任务 {job_id}。"))?;

            match job.status.as_str() {
                "waiting" | "running" => {
                    job.status = "blocked".to_string();
                    job.stage_key = "cancelled".to_string();
                    job.progress = job.progress.min(95);
                    job.error = Some("已由操作员取消。".to_string());
                    job.finished_at_ms = Some(timestamp);
                    job.updated_at_ms = timestamp;
                    job.notes.push("[cancelled] 已由操作员取消。".to_string());
                    let _ = append_stage_log(
                        Path::new(&job.artifact_dir),
                        "cancelled",
                        "已由操作员取消。",
                    );
                    Ok::<JobView, String>(JobView::from(&*job))
                }
                _ => Err("当前状态不能取消。".to_string()),
            }
        }?;

        self.queue.updated_at_ms = timestamp;
        self.save()?;
        Ok(view)
    }

    fn retry_job(&mut self, job_id: &str) -> Result<JobView, String> {
        let timestamp = now_ms();
        let view: JobView = {
            let job = self
                .queue
                .jobs
                .iter_mut()
                .find(|job| job.id == job_id)
                .ok_or_else(|| format!("找不到任务 {job_id}。"))?;

            if job.status == "running" {
                return Err("运行中的任务不能重试。".to_string());
            }

            job.status = "waiting".to_string();
            job.stage_key = "queued".to_string();
            job.progress = 0;
            job.actual_prompt_tokens = 0;
            job.actual_completion_tokens = 0;
            job.actual_cost_cny = 0.0;
            job.started_at_ms = None;
            job.finished_at_ms = None;
            job.updated_at_ms = timestamp;
            job.error = None;
            job.stage_index = 0;
            job.notes.push("[queued] 任务已重新入队。".to_string());
            let _ = append_stage_log(Path::new(&job.artifact_dir), "queued", "任务已重新入队。");
            Ok::<JobView, String>(JobView::from(&*job))
        }?;

        self.queue.updated_at_ms = timestamp;
        self.save()?;
        Ok(view)
    }
}

impl From<&JobRecord> for JobView {
    fn from(value: &JobRecord) -> Self {
        JobView {
            id: value.id.clone(),
            name: value.name.clone(),
            mode: value.mode.clone(),
            source_kind: value.source_kind.clone(),
            source_value: value.source_value.clone(),
            status: value.status.clone(),
            stage_key: value.stage_key.clone(),
            progress: value.progress,
            text_tier: value.text_tier.clone(),
            estimated_prompt_tokens: value.estimated_prompt_tokens,
            estimated_completion_tokens: value.estimated_completion_tokens,
            estimated_total_tokens: value.estimated_prompt_tokens
                + value.estimated_completion_tokens,
            actual_prompt_tokens: value.actual_prompt_tokens,
            actual_completion_tokens: value.actual_completion_tokens,
            actual_total_tokens: value.actual_prompt_tokens + value.actual_completion_tokens,
            estimated_cost_cny: value.estimated_cost_cny,
            actual_cost_cny: value.actual_cost_cny,
            effective_text_model: value.effective_text_model.clone(),
            effective_text_base_url: value.effective_text_base_url.clone(),
            effective_vision_model: value.effective_vision_model.clone(),
            created_at_ms: value.created_at_ms,
            updated_at_ms: value.updated_at_ms,
            started_at_ms: value.started_at_ms,
            finished_at_ms: value.finished_at_ms,
            artifact_dir: value.artifact_dir.clone(),
            material_pack_path: material_pack_path(Path::new(&value.artifact_dir))
                .is_file()
                .then(|| {
                    material_pack_path(Path::new(&value.artifact_dir))
                        .display()
                        .to_string()
                }),
            competitor_report_path: competitor_report_path(Path::new(&value.artifact_dir))
                .is_file()
                .then(|| {
                    competitor_report_path(Path::new(&value.artifact_dir))
                        .display()
                        .to_string()
                }),
            stage_log_path: stage_log_path(Path::new(&value.artifact_dir))
                .display()
                .to_string(),
            notes: value.notes.clone(),
            error: value.error.clone(),
        }
    }
}

impl From<&JobRecord> for JobWorkItem {
    fn from(value: &JobRecord) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            mode: value.mode.clone(),
            source_kind: value.source_kind.clone(),
            source_value: value.source_value.clone(),
            stage_key: value.stage_key.clone(),
            text_tier: value.text_tier.clone(),
            effective_text_model: value.effective_text_model.clone(),
            effective_text_base_url: value.effective_text_base_url.clone(),
            frame_count: value.frame_count,
            duration_minutes: value.duration_minutes,
            artifact_dir: PathBuf::from(&value.artifact_dir),
        }
    }
}

fn stage_keys_for_mode(mode: &str) -> &'static [&'static str] {
    match mode {
        "review" => &[
            "review_preprocess",
            "review_transcript",
            "review_vision",
            "review_text",
            "review_report",
        ],
        "competitor" => &[
            "competitor_ingest",
            "competitor_preprocess",
            "competitor_vision",
            "competitor_text",
            "competitor_report",
        ],
        _ => &[
            "extract_ingest",
            "extract_preprocess",
            "extract_ocr",
            "extract_asr",
            "extract_vision",
            "extract_text",
            "extract_material_pack",
        ],
    }
}

fn start_waiting_job(job: &mut JobRecord) {
    let stages = stage_keys_for_mode(&job.mode);
    let timestamp = now_ms();
    job.status = "running".to_string();
    job.stage_index = 0;
    job.stage_key = stages.first().copied().unwrap_or("running").to_string();
    job.started_at_ms = Some(timestamp);
    job.updated_at_ms = timestamp;
    job.notes
        .push(format!("[{}] 任务开始执行。", job.stage_key));
    let _ = append_stage_log(
        Path::new(&job.artifact_dir),
        &job.stage_key,
        "任务开始执行。",
    );
    apply_stage_share(job, 1, stages.len() + 1);
}

fn advance_running_job(job: &mut JobRecord) -> bool {
    let stages = stage_keys_for_mode(&job.mode);
    if stages.is_empty() {
        complete_job(job);
        return true;
    }

    if job.stage_index + 1 < stages.len() {
        job.stage_index += 1;
        job.stage_key = stages[job.stage_index].to_string();
        job.updated_at_ms = now_ms();
        job.notes
            .push(format!("[{}] 已进入新阶段。", job.stage_key));
        let _ = append_stage_log(
            Path::new(&job.artifact_dir),
            &job.stage_key,
            "已进入新阶段。",
        );
        apply_stage_share(job, job.stage_index + 1, stages.len() + 1);
        return true;
    }

    complete_job(job);
    true
}

fn apply_stage_share(job: &mut JobRecord, completed_steps: usize, total_steps: usize) {
    let share = (completed_steps as f64 / total_steps as f64).clamp(0.0, 0.95);
    job.progress = (share * 100.0).round() as u8;
    job.actual_prompt_tokens = (job.target_prompt_tokens as f64 * share).round() as u32;
    job.actual_completion_tokens = (job.target_completion_tokens as f64 * share).round() as u32;
    job.actual_cost_cny = round2(job.target_cost_cny * share);
}

fn complete_job(job: &mut JobRecord) {
    let timestamp = now_ms();
    job.status = "done".to_string();
    job.stage_key = "completed".to_string();
    job.progress = 100;
    job.actual_prompt_tokens = job.target_prompt_tokens;
    job.actual_completion_tokens = job.target_completion_tokens;
    job.actual_cost_cny = job.target_cost_cny;
    job.finished_at_ms = Some(timestamp);
    job.updated_at_ms = timestamp;
    if job.error.is_none() {
        let completion_note = if job.mode == "competitor" {
            "真实竞品对比链路已执行完成，报告和素材包已写入当前任务目录。"
        } else {
            "真实素材提炼链路已执行完成，产物已写入当前任务目录。"
        };
        job.notes.push(completion_note.to_string());
        let _ = append_stage_log(Path::new(&job.artifact_dir), "completed", completion_note);
    }
}

fn worker_loop(shared: Arc<Mutex<JobStore>>) {
    thread::spawn(move || loop {
        let next_item = {
            let mut guard = match shared.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    thread::sleep(Duration::from_millis(JOB_POLL_INTERVAL_MS));
                    continue;
                }
            };
            match guard.prepare_next_work_item() {
                Ok(item) => item,
                Err(err) => {
                    eprintln!("job worker scheduling failed: {err}");
                    None
                }
            }
        };

        let Some(item) = next_item else {
            thread::sleep(Duration::from_millis(JOB_POLL_INTERVAL_MS));
            continue;
        };

        match process_job_stage(&item) {
            Ok(notes) => {
                if let Ok(mut guard) = shared.lock() {
                    if let Err(err) = guard.finish_stage(&item.id, notes) {
                        eprintln!("job worker commit failed for {}: {err}", item.id);
                    }
                }
            }
            Err(err) => {
                if let Ok(mut guard) = shared.lock() {
                    if let Err(commit_err) = guard.fail_job(&item.id, err.clone()) {
                        eprintln!(
                            "job worker fail commit failed for {}: {commit_err}",
                            item.id
                        );
                    }
                }
                eprintln!("job worker stage failed for {}: {err}", item.id);
            }
        }
    });
}

#[tauri::command]
fn get_runtime_settings() -> Result<RuntimeSettingsView, String> {
    let envelope = load_settings_envelope()?;
    to_settings_view(&envelope)
}

#[tauri::command]
fn save_runtime_settings(update: RuntimeSettingsUpdate) -> Result<(), String> {
    let current = load_settings_envelope()?;
    let merged = apply_settings_update(current, update);
    save_settings_envelope(&merged)
}

#[tauri::command]
fn estimate_job_cost(request: EstimateJobRequest) -> Result<EstimateJobResult, String> {
    let settings = load_settings_envelope()?;
    Ok(estimate_job_cost_impl(&settings.settings, &request))
}

#[tauri::command]
fn list_jobs(state: tauri::State<'_, AppState>) -> Result<Vec<JobView>, String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    Ok(guard.list_jobs())
}

#[tauri::command]
fn get_job(job_id: String, state: tauri::State<'_, AppState>) -> Result<Option<JobView>, String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    Ok(guard.get_job(&job_id))
}

#[tauri::command]
fn create_job(
    request: CreateJobRequest,
    state: tauri::State<'_, AppState>,
) -> Result<JobView, String> {
    let settings = load_settings_envelope()?;
    let mut guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    guard.create_job(request, &settings.settings)
}

#[tauri::command]
fn cancel_job(job_id: String, state: tauri::State<'_, AppState>) -> Result<JobView, String> {
    let mut guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    guard.cancel_job(&job_id)
}

#[tauri::command]
fn retry_job(job_id: String, state: tauri::State<'_, AppState>) -> Result<JobView, String> {
    let mut guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    guard.retry_job(&job_id)
}

#[tauri::command]
fn get_dashboard_snapshot(state: tauri::State<'_, AppState>) -> Result<DashboardSnapshot, String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    Ok(guard.dashboard())
}

#[tauri::command]
fn read_job_stage_log(job_id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    guard.read_stage_log(&job_id)
}

#[tauri::command]
fn read_job_material_pack(
    job_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    guard.read_material_pack(&job_id)
}

#[tauri::command]
fn read_job_competitor_report(
    job_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    guard.read_competitor_report(&job_id)
}

#[tauri::command]
fn generate_material_prompt(
    request: MaterialPromptRewriteRequest,
) -> Result<MaterialPromptRewriteResult, String> {
    let settings = load_settings_envelope()?.settings;
    let endpoint = build_text_endpoint_for_tier(&settings, &request.text_tier)?;
    request_material_prompt_rewrite(&endpoint, &settings, &request)
}

#[tauri::command]
fn check_runtime_environment() -> Result<EnvironmentHealthReport, String> {
    let settings = load_settings_envelope()?.settings;
    build_environment_report(&settings)
}

#[tauri::command]
fn open_environment_setup_script() -> Result<String, String> {
    let path = ensure_windows_setup_script()?;
    open_in_explorer(&path, true)?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn open_job_artifact_dir(job_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    let path = guard.job_artifact_dir(&job_id)?;
    drop(guard);
    open_in_explorer(&path, false)
}

#[tauri::command]
fn open_job_material_pack(job_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| "job queue lock poisoned".to_string())?;
    let path = guard.job_material_pack_path(&job_id)?;
    drop(guard);
    open_in_explorer(&path, true)
}

#[tauri::command]
fn open_runtime_settings_dir() -> Result<(), String> {
    let dir = app_config_dir()?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_in_explorer(&dir, false)
}

pub fn run() {
    let store = JobStore::load_or_create().unwrap_or_else(|err| {
        panic!("failed to initialize job store: {err}");
    });
    let shared = Arc::new(Mutex::new(store));
    worker_loop(shared.clone());

    tauri::Builder::default()
        .manage(AppState { jobs: shared })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_runtime_settings,
            save_runtime_settings,
            estimate_job_cost,
            list_jobs,
            get_job,
            create_job,
            cancel_job,
            retry_job,
            get_dashboard_snapshot,
            read_job_stage_log,
            read_job_material_pack,
            read_job_competitor_report,
            generate_material_prompt,
            check_runtime_environment,
            open_environment_setup_script,
            open_job_artifact_dir,
            open_job_material_pack,
            open_runtime_settings_dir
        ])
        .setup(|app| {
            prepare_bundled_runtime(app.handle()).map_err(io::Error::other)?;
            load_settings_envelope()
                .map(|_| ())
                .map_err(io::Error::other)?;
            load_usage_ledger().map(|_| ()).map_err(io::Error::other)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running MicrocodeX Short Video Workbench");
}

#[cfg(test)]
mod tests {
    use super::{
        extract_douyin_url_from_share_text, latest_downloaded_video,
        summarize_douyin_download_issue,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "mcx_short_video_{name}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        dir
    }

    #[test]
    fn extracts_clean_url_from_share_text() {
        let source = "5.89 复制打开抖音，看看【编导厉岑的作品】自制AIGC动画《余烬之后》 https://v.douyin.com/9b-SVijteYw/ k@c.NW Btr:/ 08/01 :1pm";
        let extracted = extract_douyin_url_from_share_text(source);
        assert_eq!(
            extracted.as_deref(),
            Some("https://v.douyin.com/9b-SVijteYw/")
        );
    }

    #[test]
    fn normalizes_short_url_without_scheme() {
        let source = "复制后打开 v.douyin.com/mcsNoHv8Hlc/ 看看";
        let extracted = extract_douyin_url_from_share_text(source);
        assert_eq!(
            extracted.as_deref(),
            Some("https://v.douyin.com/mcsNoHv8Hlc/")
        );
    }

    #[test]
    fn surfaces_parse_failures_from_downloader_output() {
        let output = "Found 1 URL(s) to process\nFailed to parse URL: 5.89 分享文案";
        let message = summarize_douyin_download_issue(output);
        assert!(message.is_some());
        assert!(message.unwrap().contains("解析出抖音链接"));
    }

    #[test]
    fn latest_downloaded_video_requires_a_new_file() {
        let root = unique_temp_dir("latest_downloaded_video");
        fs::create_dir_all(&root).unwrap();
        let old_video = root.join("old.mp4");
        fs::write(&old_video, b"old").unwrap();
        thread::sleep(Duration::from_millis(20));
        let started_at = SystemTime::now();
        let result = latest_downloaded_video(&root, started_at);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
