//! Keyframe extraction for the video→system-prompt step.
//!
//! Reliability first: prefer scene-change frames (distinct shots make better
//! reverse-prompt material), but always fall back to uniform time sampling so a
//! static clip — or one whose scene filter yields nothing — never returns zero
//! frames. Extracted frames are JPEG-encoded, exact-duplicate-deduped, and
//! capped in both count and width to bound downstream VLM token cost.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::media::validate_video_file_l0;
use crate::{Result, VideoAgentError};

/// Width cap (pixels) applied to VL frames; height keeps aspect ratio.
pub const DEFAULT_MAX_WIDTH: i64 = 768;
/// Higher width cap for a dedicated OCR pass, where small on-screen subtitle text
/// must stay legible (the 768px VL frames are too small for reliable OCR).
pub const OCR_MAX_WIDTH: i64 = 1280;
/// Scene-change score threshold used in `select='gt(scene,THRESHOLD)'`.
pub const DEFAULT_SCENE_THRESHOLD: f64 = 0.4;
/// Hard ceiling on returned frames regardless of the requested count.
pub const MAX_FRAMES: usize = 60;

/// Extract up to `n` representative JPEG frames at the default VL width (768px).
///
/// The result is deterministic for a given input and never empty on success:
/// scene-change detection is tried first, and uniform time sampling is used as a
/// fallback (and to guarantee coverage). Exact-duplicate frames are removed and
/// the set is downsampled evenly to at most `n`.
pub fn extract_keyframes(path: impl AsRef<Path>, n: usize) -> Result<Vec<Vec<u8>>> {
    extract_keyframes_scaled(path, n, DEFAULT_MAX_WIDTH)
}

/// Like [`extract_keyframes`] but with an explicit width cap. Use
/// [`OCR_MAX_WIDTH`] for a subtitle/OCR pass where text legibility matters more
/// than token cost.
pub fn extract_keyframes_scaled(
    path: impl AsRef<Path>,
    n: usize,
    max_width: i64,
) -> Result<Vec<Vec<u8>>> {
    let path = path.as_ref();
    let n = n.clamp(1, MAX_FRAMES);

    // L0 gate: reuse the existing probe so a broken/empty file is refused early.
    let report = validate_video_file_l0(path, None, 0.0, false)?;
    if !report.passed {
        return Err(VideoAgentError::Ffmpeg(format!(
            "keyframe extraction refused; media L0 failed: {}",
            report.reasons.join("; ")
        )));
    }
    let probe = report
        .probe
        .ok_or_else(|| VideoAgentError::Ffmpeg("media L0 passed without a probe".to_string()))?;
    let scale = probe
        .width
        .filter(|w| *w > max_width)
        .map(|_| max_width);

    // Prefer scene-change frames; fall back to uniform sampling if too few.
    let mut frames = scene_keyframes(path, n, scale).unwrap_or_default();
    if frames.len() < 2 {
        if let Some(duration) = probe.duration_s.filter(|d| *d > 0.0) {
            frames = uniform_keyframes(path, n, duration, scale)?;
        }
    }

    let frames = dedup_exact(frames);
    if frames.is_empty() {
        return Err(VideoAgentError::Ffmpeg(
            "no frames could be extracted from the video".to_string(),
        ));
    }
    Ok(downsample(frames, n))
}

/// Scan the whole clip for scene changes and dump one JPEG per detected cut.
fn scene_keyframes(path: &Path, cap: usize, scale: Option<i64>) -> Result<Vec<Vec<u8>>> {
    let dir = TempDir::new("scene")?;
    let mut vf = format!("select='gt(scene,{DEFAULT_SCENE_THRESHOLD})'");
    if let Some(w) = scale {
        vf.push_str(&format!(",scale={w}:-2"));
    }
    let pattern = dir.path.join("scene_%04d.jpg");
    // Grab extra so even downsampling has material; still bounded for long clips.
    let frames_cap = cap.saturating_mul(3).clamp(1, MAX_FRAMES * 3);

    let output = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .arg("-i")
        .arg(path)
        .arg("-vf")
        .arg(&vf)
        .args(["-vsync", "vfr"])
        .arg("-frames:v")
        .arg(frames_cap.to_string())
        .args(["-q:v", "3"])
        .arg(&pattern)
        .output()
        .map_err(|err| VideoAgentError::Ffmpeg(format!("failed to launch ffmpeg (scene): {err}")))?;
    if !output.status.success() {
        return Err(VideoAgentError::Ffmpeg(format!(
            "ffmpeg scene select failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    read_jpegs(&dir.path)
}

/// Sample `n` frames at evenly spaced interior timestamps (duration/(n+1) steps).
fn uniform_keyframes(
    path: &Path,
    n: usize,
    duration: f64,
    scale: Option<i64>,
) -> Result<Vec<Vec<u8>>> {
    let dir = TempDir::new("uniform")?;
    let mut frames = Vec::with_capacity(n);
    for i in 0..n {
        let timestamp = duration * (i as f64 + 1.0) / (n as f64 + 1.0);
        let out = dir.path.join(format!("u_{i:03}.jpg"));

        let mut command = Command::new("ffmpeg");
        command
            .args(["-y", "-v", "error"])
            .arg("-ss")
            .arg(format!("{timestamp:.3}"))
            .arg("-i")
            .arg(path)
            .args(["-frames:v", "1"]);
        if let Some(w) = scale {
            command.arg("-vf").arg(format!("scale={w}:-2"));
        }
        let output = command
            .args(["-q:v", "3"])
            .arg(&out)
            .output()
            .map_err(|err| {
                VideoAgentError::Ffmpeg(format!("failed to launch ffmpeg (uniform): {err}"))
            })?;
        if !output.status.success() {
            return Err(VideoAgentError::Ffmpeg(format!(
                "ffmpeg uniform sample at {timestamp:.3}s failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        if let Ok(bytes) = fs::read(&out) {
            if !bytes.is_empty() {
                frames.push(bytes);
            }
        }
    }
    Ok(frames)
}

fn read_jpegs(dir: &Path) -> Result<Vec<Vec<u8>>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|err| {
            VideoAgentError::Ffmpeg(format!("read frame dir {} failed: {err}", dir.display()))
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jpg"))
        .collect();
    entries.sort();

    let mut frames = Vec::with_capacity(entries.len());
    for path in entries {
        if let Ok(bytes) = fs::read(&path) {
            if !bytes.is_empty() {
                frames.push(bytes);
            }
        }
    }
    Ok(frames)
}

fn dedup_exact(frames: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut seen: HashSet<[u8; 32]> = HashSet::new();
    let mut out = Vec::with_capacity(frames.len());
    for frame in frames {
        let mut hasher = Sha256::new();
        hasher.update(&frame);
        let key: [u8; 32] = hasher.finalize().into();
        if seen.insert(key) {
            out.push(frame);
        }
    }
    out
}

fn downsample(frames: Vec<Vec<u8>>, n: usize) -> Vec<Vec<u8>> {
    let total = frames.len();
    if total <= n {
        return frames;
    }
    (0..n).map(|i| frames[i * total / n].clone()).collect()
}

/// Temp directory that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "ncx-keyframes-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).map_err(|err| {
            VideoAgentError::Ffmpeg(format!("create temp dir {} failed: {err}", path.display()))
        })?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg").arg("-version").output().is_ok()
            && Command::new("ffprobe").arg("-version").output().is_ok()
    }

    fn make_test_clip(path: &Path) {
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=10:duration=1",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(path)
            .status()
            .expect("run ffmpeg");
        assert!(status.success(), "test clip generation failed");
    }

    #[test]
    fn extract_keyframes_returns_bounded_nonempty_jpegs() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg/ffprobe not available; skipping keyframe smoke");
            return;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let clip = std::env::temp_dir().join(format!("ncx-keyframes-test-{nanos}.mp4"));
        make_test_clip(&clip);

        let frames = extract_keyframes(&clip, 6).expect("extract keyframes");
        assert!(!frames.is_empty(), "expected at least one frame");
        assert!(frames.len() <= 6, "must respect the requested cap");
        for frame in &frames {
            // JPEG SOI marker; confirms we read real image bytes, not garbage.
            assert_eq!(&frame[..2], &[0xFF, 0xD8], "frame is not a JPEG");
        }

        let _ = fs::remove_file(clip);
    }

    #[test]
    fn extract_keyframes_rejects_missing_file() {
        let missing = std::env::temp_dir().join("ncx-keyframes-does-not-exist.mp4");
        let err = extract_keyframes(&missing, 4).unwrap_err();
        assert!(matches!(err, VideoAgentError::Ffmpeg(_)));
    }

    #[test]
    fn dedup_exact_collapses_identical_frames() {
        let frames = vec![vec![1, 2, 3], vec![1, 2, 3], vec![4, 5, 6]];
        let out = dedup_exact(frames);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn downsample_picks_evenly_and_caps() {
        let frames: Vec<Vec<u8>> = (0..10u8).map(|i| vec![i]).collect();
        let out = downsample(frames, 4);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], vec![0]);
    }
}
