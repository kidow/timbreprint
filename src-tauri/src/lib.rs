use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{AppHandle, Manager};

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("audio file is larger than the MVP limit of 10 minutes")]
    TooLong,
    #[error("ffmpeg was not found. Install ffmpeg or add it to PATH.")]
    MissingFfmpeg,
    #[error("ffmpeg conversion failed: {0}")]
    FfmpegFailed(String),
    #[error("python was not found. Install python3 or add it to PATH.")]
    MissingPython,
    #[error("python worker was not found: {0}")]
    MissingWorker(String),
    #[error("python worker failed: {0}")]
    WorkerFailed(String),
    #[error("failed to prepare job directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to serialize JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to resolve app data directory")]
    AppDataDir,
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolStatus {
    ffmpeg: Option<String>,
    python: Option<String>,
    app_data_dir: String,
    models_dir: String,
    logs_dir: String,
}

#[derive(Deserialize, Serialize)]
struct ScoredValue<T> {
    value: T,
    confidence: f32,
}

#[derive(Deserialize, Serialize)]
struct Analysis {
    tempo: ScoredValue<u16>,
    key: ScoredValue<String>,
    energy: ScoredValue<String>,
    mood: Vec<ScoredValue<String>>,
    genre: Vec<ScoredValue<String>>,
    instruments: Vec<ScoredValue<String>>,
    texture: Vec<ScoredValue<String>>,
    #[serde(default)]
    rhythm: Vec<ScoredValue<String>>,
    #[serde(default)]
    dynamics: Vec<ScoredValue<String>>,
    #[serde(default)]
    brightness: Vec<ScoredValue<String>>,
    #[serde(default)]
    space: Vec<ScoredValue<String>>,
    #[serde(default)]
    arrangement: Vec<ScoredValue<String>>,
    #[serde(default, rename = "negativePrompt")]
    negative_prompt: Vec<ScoredValue<String>>,
    #[serde(default)]
    features: Option<AnalysisFeatures>,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct AnalysisFeatures {
    analysis_backend: Option<String>,
    duration_seconds: Option<f32>,
    rms: Option<f32>,
    zero_crossing_rate: Option<f32>,
    spectral_centroid_hz: Option<f32>,
    onset_density: Option<f32>,
    tagging_model_status: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobResult {
    id: String,
    status: &'static str,
    source_path: String,
    job_dir: String,
    analysis: Analysis,
    prompt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobState<'a> {
    id: &'a str,
    status: &'a str,
    source_path: &'a str,
    processed_path: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceMetadata<'a> {
    original_file_name: String,
    source_path: &'a str,
    extension: String,
    size_bytes: u64,
    duration_seconds: Option<u64>,
}

#[tauri::command]
fn check_environment(app: AppHandle) -> Result<ToolStatus, AppError> {
    let app_data_dir = app_data_dir(&app)?;
    let logs_dir = app_data_dir.join("logs");
    let models_dir = app_data_dir.join("models");
    fs::create_dir_all(&logs_dir)?;
    fs::create_dir_all(&models_dir)?;

    Ok(ToolStatus {
        ffmpeg: find_executable("ffmpeg"),
        python: worker_python_path()
            .or_else(|| find_executable("python3").or_else(|| find_executable("python"))),
        app_data_dir: app_data_dir.display().to_string(),
        models_dir: models_dir.display().to_string(),
        logs_dir: logs_dir.display().to_string(),
    })
}

#[tauri::command]
fn run_analysis(app: AppHandle, source_path: String) -> Result<JobResult, AppError> {
    validate_audio_path(&source_path)?;

    let source = PathBuf::from(&source_path);
    let duration_seconds = probe_duration_seconds(&source);
    if duration_seconds.is_some_and(|duration| duration > 600) {
        return Err(AppError::TooLong);
    }

    let app_data_dir = app_data_dir(&app)?;
    let jobs_dir = app_data_dir.join("jobs");
    let models_dir = app_data_dir.join("models");
    fs::create_dir_all(&jobs_dir)?;
    fs::create_dir_all(&models_dir)?;

    let file_stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("track");
    let safe_name = sanitize_name(file_stem);
    let id = format!("{}_{}", Local::now().format("%Y-%m-%d_%H%M%S"), safe_name);
    let job_dir = jobs_dir.join(&id);
    fs::create_dir_all(&job_dir)?;

    let processed_path = job_dir.join("processed.wav");
    convert_to_wav(&source, &processed_path, &job_dir)?;

    let metadata = fs::metadata(&source)?;
    let source_metadata = SourceMetadata {
        original_file_name: source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string(),
        source_path: &source_path,
        extension: source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase(),
        size_bytes: metadata.len(),
        duration_seconds,
    };
    write_json(job_dir.join("source-metadata.json"), &source_metadata)?;

    run_python_worker(&job_dir, &models_dir)?;
    let analysis = read_analysis(&job_dir)?;
    let prompt = generate_prompt(&analysis);

    let job_state = JobState {
        id: &id,
        status: "completed",
        source_path: &source_path,
        processed_path: processed_path.display().to_string(),
        created_at: Local::now().to_rfc3339(),
    };

    write_json(job_dir.join("job.json"), &job_state)?;
    fs::write(job_dir.join("prompt.txt"), &prompt)?;

    Ok(JobResult {
        id,
        status: "completed",
        source_path,
        job_dir: job_dir.display().to_string(),
        analysis,
        prompt,
    })
}

#[tauri::command]
fn open_path(path: String) -> Result<(), AppError> {
    Command::new("open").arg(path).spawn()?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            check_environment,
            run_analysis,
            open_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running Timbreprint");
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path().app_data_dir().map_err(|_| AppError::AppDataDir)
}

fn validate_audio_path(path: &str) -> Result<(), AppError> {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "mp3" | "wav" | "m4a" | "flac" => Ok(()),
        _ => Err(AppError::UnsupportedFormat(extension)),
    }
}

fn find_executable(name: &str) -> Option<String> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            } else {
                None
            }
        })
}

fn probe_duration_seconds(path: &Path) -> Option<u64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    value
        .trim()
        .parse::<f64>()
        .ok()
        .map(|seconds| seconds.ceil() as u64)
}

fn convert_to_wav(source: &Path, output_path: &Path, job_dir: &Path) -> Result<(), AppError> {
    let ffmpeg = find_executable("ffmpeg").ok_or(AppError::MissingFfmpeg)?;
    let output = Command::new(ffmpeg)
        .args(["-y", "-v", "error", "-i"])
        .arg(source)
        .args(["-vn", "-ac", "2", "-ar", "44100"])
        .arg(output_path)
        .output()?;

    let log_path = job_dir.join("ffmpeg.log");
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let log_body = format!("stdout:\n{stdout}\n\nstderr:\n{stderr}\n");
    fs::write(log_path, log_body)?;

    if !output.status.success() {
        return Err(AppError::FfmpegFailed(if stderr.is_empty() {
            "unknown ffmpeg error".to_string()
        } else {
            stderr
        }));
    }

    Ok(())
}

fn run_python_worker(job_dir: &Path, models_dir: &Path) -> Result<(), AppError> {
    let python = worker_python_path()
        .or_else(|| find_executable("python3").or_else(|| find_executable("python")))
        .ok_or(AppError::MissingPython)?;
    let worker_path = worker_script_path()?;

    let output = Command::new(python)
        .arg(worker_path)
        .arg(job_dir)
        .arg(models_dir)
        .output()?;

    let log_path = job_dir.join("worker.log");
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let log_body = format!("stdout:\n{stdout}\n\nstderr:\n{stderr}\n");
    fs::write(log_path, log_body)?;

    if !output.status.success() {
        return Err(AppError::WorkerFailed(if stderr.is_empty() {
            "unknown worker error".to_string()
        } else {
            stderr
        }));
    }

    Ok(())
}

fn worker_python_path() -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../workers/.venv/bin/python")
        .canonicalize()
        .ok()?;

    if path.is_file() {
        Some(path.display().to_string())
    } else {
        None
    }
}

fn worker_script_path() -> Result<PathBuf, AppError> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../workers/audio_analysis.py")
        .canonicalize()
        .map_err(|_| {
            AppError::MissingWorker(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../workers/audio_analysis.py")
                    .display()
                    .to_string(),
            )
        })?;
    Ok(path)
}

fn read_analysis(job_dir: &Path) -> Result<Analysis, AppError> {
    let body = fs::read_to_string(job_dir.join("analysis.json"))?;
    Ok(serde_json::from_str(&body)?)
}

fn sanitize_name(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    output.truncate(40);
    output.trim_matches('-').to_string()
}

fn generate_prompt(analysis: &Analysis) -> String {
    let mut opening = "Create an original music track".to_string();

    if analysis.tempo.value > 0 && analysis.tempo.confidence >= 0.35 {
        opening.push_str(&format!(" around {} BPM", analysis.tempo.value));
    }

    if is_known_value(&analysis.key.value) && analysis.key.confidence >= 0.3 {
        opening.push_str(&format!(" centered around {}", analysis.key.value));
    }

    let mut sentences = vec![format!("{opening}.")];

    let genre = join_confident_values(&analysis.genre, 0.3);
    let texture = join_confident_values(&analysis.texture, 0.35);
    let energy = if is_known_value(&analysis.energy.value) && analysis.energy.confidence >= 0.4 {
        Some(analysis.energy.value.as_str())
    } else {
        None
    };
    if !genre.is_empty() || !texture.is_empty() || energy.is_some() {
        sentences.push(format!(
            "Style direction: {}{}{}.",
            phrase_or_default(&genre, "original contemporary production"),
            phrase_suffix(&texture, " with a ", " texture"),
            energy.map_or_else(String::new, |value| format!(" at {value} energy"))
        ));
    }

    let mood = join_confident_values(&analysis.mood, 0.35);
    if !mood.is_empty() {
        sentences.push(format!("Mood: keep it {mood}."));
    }

    let arrangement = join_confident_values(&analysis.arrangement, 0.35);
    let rhythm = join_confident_values(&analysis.rhythm, 0.35);
    if !arrangement.is_empty() || !rhythm.is_empty() {
        sentences.push(format!(
            "Arrangement: {}{}.",
            phrase_or_default(&arrangement, "keep the structure clear and focused"),
            phrase_suffix(&rhythm, " with ", "")
        ));
    }

    let instruments = join_confident_values(&analysis.instruments, 0.25);
    let brightness = join_confident_values(&analysis.brightness, 0.35);
    if !instruments.is_empty() || !brightness.is_empty() {
        sentences.push(format!(
            "Sound palette: {}{}.",
            phrase_or_default(&instruments, "use a cohesive instrumental palette"),
            phrase_suffix(&brightness, " with ", "")
        ));
    }

    let space = join_confident_values(&analysis.space, 0.35);
    let dynamics = join_confident_values(&analysis.dynamics, 0.35);
    let mix_phrase = mix_phrase(analysis);
    if !space.is_empty() || !dynamics.is_empty() || !mix_phrase.is_empty() {
        sentences.push(format!(
            "Mix direction: {}{}{}.",
            phrase_or_default(&space, "keep the mix clean and usable"),
            phrase_suffix(&dynamics, " with ", ""),
            phrase_suffix(&mix_phrase, ", ", "")
        ));
    }

    let negative_prompt = join_confident_values(&analysis.negative_prompt, 0.4);
    if negative_prompt.is_empty() {
        sentences.push(
            "Avoid artist-specific references, recreating existing songs, or recognizable copyrighted melody."
                .to_string(),
        );
    } else {
        sentences.push(format!("Avoid {negative_prompt}."));
    }

    sanitize_prompt(&sentences.join(" "))
}

fn join_confident_values(items: &[ScoredValue<String>], minimum_confidence: f32) -> String {
    items
        .iter()
        .filter(|item| item.confidence >= minimum_confidence && is_known_value(&item.value))
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_known_value(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    !normalized.is_empty() && normalized != "unknown"
}

fn sanitize_prompt(input: &str) -> String {
    let replacements = [
        ("in the style of", "with abstract musical traits from"),
        ("copying", "recreating"),
        ("copy", "recreate"),
        ("cloning", "recreating"),
        ("clone", "recreate"),
        ("replicating", "recreating"),
        ("replicate", "recreate"),
        ("replication", "recreation"),
    ];

    let mut output = input.to_string();
    for (needle, replacement) in replacements {
        output = replace_prompt_term(&output, needle, replacement);
    }
    collapse_prompt_spacing(&output)
}

fn replace_prompt_term(input: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return input.to_string();
    }

    let mut output = String::new();
    let input_lower = input.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut cursor = 0;

    while let Some(relative_index) = input_lower[cursor..].find(&needle_lower) {
        let start = cursor + relative_index;
        let end = start + needle.len();
        if !has_prompt_term_boundaries(input, start, end) {
            output.push_str(&input[cursor..end]);
            cursor = end;
            continue;
        }
        output.push_str(&input[cursor..start]);
        output.push_str(replacement);
        cursor = end;
    }

    output.push_str(&input[cursor..]);
    output
}

#[cfg(test)]
fn contains_prompt_term(input: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    let input_lower = input.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut cursor = 0;

    while let Some(relative_index) = input_lower[cursor..].find(&needle_lower) {
        let start = cursor + relative_index;
        let end = start + needle.len();
        if has_prompt_term_boundaries(input, start, end) {
            return true;
        }
        cursor = end;
    }

    false
}

fn has_prompt_term_boundaries(input: &str, start: usize, end: usize) -> bool {
    let before = input[..start].chars().next_back();
    let after = input[end..].chars().next();
    !is_prompt_word_character(before) && !is_prompt_word_character(after)
}

fn is_prompt_word_character(character: Option<char>) -> bool {
    character.is_some_and(|value| value.is_ascii_alphanumeric())
}

fn collapse_prompt_spacing(input: &str) -> String {
    let mut output = input.split_whitespace().collect::<Vec<_>>().join(" ");
    output = output.replace(" ,", ",");
    output = output.replace(" .", ".");
    output
}

fn phrase_or_default(value: &str, default: &str) -> String {
    if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

fn phrase_suffix(value: &str, prefix: &str, suffix: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        format!("{prefix}{value}{suffix}")
    }
}

fn mix_phrase(analysis: &Analysis) -> String {
    let Some(features) = &analysis.features else {
        return String::new();
    };

    let mut details = Vec::new();

    if let Some(spectral_centroid) = features.spectral_centroid_hz {
        if spectral_centroid >= 3200.0 {
            details.push("emphasize bright upper-frequency detail");
        } else if spectral_centroid <= 1400.0 {
            details.push("keep the tone smooth and low-centered");
        }
    }

    if let Some(onset_density) = features.onset_density {
        if onset_density >= 0.25 {
            details.push("use active rhythmic motion");
        } else if onset_density > 0.0 && onset_density < 0.08 {
            details.push("leave space between attacks");
        }
    }

    if let Some(rms) = features.rms {
        if rms < 0.035 {
            details.push("keep dynamics restrained");
        } else if rms > 0.11 {
            details.push("make the arrangement feel bold and forward");
        }
    }

    details.join(", ")
}

fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<(), AppError> {
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{body}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_audio_to_processed_wav_when_ffmpeg_is_available() {
        if find_executable("ffmpeg").is_none() {
            return;
        }

        let test_dir = std::env::temp_dir().join(format!(
            "timbreprint-ffmpeg-test-{}",
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&test_dir).expect("create test dir");

        let source_path = test_dir.join("source.wav");
        let processed_path = test_dir.join("processed.wav");

        let generated = Command::new(find_executable("ffmpeg").expect("ffmpeg path"))
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.1",
            ])
            .arg(&source_path)
            .status()
            .expect("generate source audio");
        assert!(generated.success());

        convert_to_wav(&source_path, &processed_path, &test_dir).expect("convert source audio");

        let header = fs::read(&processed_path).expect("read processed wav");
        assert!(header.starts_with(b"RIFF"));
        assert!(test_dir.join("ffmpeg.log").exists());

        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn python_worker_writes_analysis_json_when_python_is_available() {
        if find_executable("python3")
            .or_else(|| find_executable("python"))
            .is_none()
            || find_executable("ffmpeg").is_none()
        {
            return;
        }

        let test_dir = std::env::temp_dir().join(format!(
            "timbreprint-worker-test-{}",
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&test_dir).expect("create test dir");
        let generated = Command::new(find_executable("ffmpeg").expect("ffmpeg path"))
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
            ])
            .arg(test_dir.join("processed.wav"))
            .status()
            .expect("generate processed wav");
        assert!(generated.success());

        run_python_worker(&test_dir, &test_dir.join("models")).expect("run python worker");
        let analysis = read_analysis(&test_dir).expect("read analysis");

        assert!(matches!(
            analysis.energy.value.as_str(),
            "low" | "medium" | "high"
        ));
        assert!(!analysis.texture.is_empty());
        assert!(test_dir.join("worker.log").exists());

        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn prompt_omits_unknown_low_confidence_values() {
        let analysis = Analysis {
            tempo: ScoredValue {
                value: 0,
                confidence: 0.15,
            },
            key: ScoredValue {
                value: "unknown".to_string(),
                confidence: 0.1,
            },
            energy: ScoredValue {
                value: "medium".to_string(),
                confidence: 0.7,
            },
            mood: vec![ScoredValue {
                value: "steady".to_string(),
                confidence: 0.45,
            }],
            genre: vec![ScoredValue {
                value: "instrumental".to_string(),
                confidence: 0.25,
            }],
            instruments: vec![ScoredValue {
                value: "synth layers".to_string(),
                confidence: 0.25,
            }],
            texture: vec![ScoredValue {
                value: "smooth".to_string(),
                confidence: 0.55,
            }],
            rhythm: vec![ScoredValue {
                value: "spacious rhythm".to_string(),
                confidence: 0.58,
            }],
            dynamics: vec![ScoredValue {
                value: "restrained dynamics".to_string(),
                confidence: 0.64,
            }],
            brightness: vec![ScoredValue {
                value: "dark low-mid focus".to_string(),
                confidence: 0.62,
            }],
            space: vec![ScoredValue {
                value: "wide atmospheric space".to_string(),
                confidence: 0.55,
            }],
            arrangement: vec![ScoredValue {
                value: "start minimal and let layers enter gradually".to_string(),
                confidence: 0.46,
            }],
            negative_prompt: vec![
                ScoredValue {
                    value: "artist-specific references".to_string(),
                    confidence: 0.9,
                },
                ScoredValue {
                    value: "recreating existing songs".to_string(),
                    confidence: 0.9,
                },
            ],
            features: Some(AnalysisFeatures {
                analysis_backend: Some("librosa".to_string()),
                duration_seconds: Some(12.0),
                rms: Some(0.02),
                zero_crossing_rate: Some(0.01),
                spectral_centroid_hz: Some(900.0),
                onset_density: Some(0.03),
                tagging_model_status: None,
            }),
        };

        let prompt = generate_prompt(&analysis);

        assert!(!prompt.contains("0 BPM"));
        assert!(!prompt.contains("unknown"));
        assert!(prompt.contains("Arrangement: start minimal"));
        assert!(prompt.contains("Sound palette: synth layers"));
        assert!(prompt.contains("Mix direction: wide atmospheric space"));
        assert!(prompt.contains("keep the tone smooth and low-centered"));
        assert!(prompt.contains("Avoid artist-specific references"));
    }

    #[test]
    fn prompt_sanitizer_rewrites_direct_imitation_terms() {
        let prompt = sanitize_prompt(
            "Make this in the style of a famous artist, copy the hook, clone the groove, and replicate the drop.",
        );

        assert_prompt_policy(&prompt);
        assert!(prompt
            .to_ascii_lowercase()
            .contains("abstract musical traits"));
        assert!(prompt.to_ascii_lowercase().contains("recreate"));
    }

    #[test]
    fn prompt_sanitizer_preserves_copyrighted_safety_phrase() {
        let prompt = sanitize_prompt(
            "Avoid artist-specific references, recreating existing songs, or recognizable copyrighted melody.",
        );

        assert_prompt_policy(&prompt);
        assert!(prompt.contains("recognizable copyrighted melody"));
        assert!(!prompt.contains("recreaterighted"));
    }

    #[test]
    fn generated_prompt_uses_default_safety_policy_when_negative_prompt_is_empty() {
        let analysis = Analysis {
            tempo: ScoredValue {
                value: 120,
                confidence: 0.8,
            },
            key: ScoredValue {
                value: "C tonal center".to_string(),
                confidence: 0.5,
            },
            energy: ScoredValue {
                value: "high".to_string(),
                confidence: 0.7,
            },
            mood: vec![],
            genre: vec![ScoredValue {
                value: "electronic".to_string(),
                confidence: 0.5,
            }],
            instruments: vec![],
            texture: vec![],
            rhythm: vec![],
            dynamics: vec![],
            brightness: vec![],
            space: vec![],
            arrangement: vec![],
            negative_prompt: vec![],
            features: None,
        };

        let prompt = generate_prompt(&analysis);

        assert_prompt_policy(&prompt);
        assert!(prompt.contains("Avoid artist-specific references"));
        assert!(prompt.contains("recreating existing songs"));
        assert!(prompt.contains("recognizable copyrighted melody"));
    }

    #[test]
    fn generated_prompt_sanitizes_unsafe_analysis_values() {
        let analysis = Analysis {
            tempo: ScoredValue {
                value: 100,
                confidence: 0.8,
            },
            key: ScoredValue {
                value: "unknown".to_string(),
                confidence: 0.1,
            },
            energy: ScoredValue {
                value: "medium".to_string(),
                confidence: 0.7,
            },
            mood: vec![ScoredValue {
                value: "copy the original mood".to_string(),
                confidence: 0.8,
            }],
            genre: vec![ScoredValue {
                value: "in the style of famous synth pop".to_string(),
                confidence: 0.8,
            }],
            instruments: vec![ScoredValue {
                value: "clone the exact lead sound".to_string(),
                confidence: 0.8,
            }],
            texture: vec![ScoredValue {
                value: "replicate the mix texture".to_string(),
                confidence: 0.8,
            }],
            rhythm: vec![],
            dynamics: vec![],
            brightness: vec![],
            space: vec![],
            arrangement: vec![],
            negative_prompt: vec![],
            features: None,
        };

        let prompt = generate_prompt(&analysis);

        assert_prompt_policy(&prompt);
        assert!(prompt.contains("recognizable copyrighted melody"));
    }

    fn assert_prompt_policy(prompt: &str) {
        for blocked in [
            "in the style of",
            "copy",
            "copying",
            "clone",
            "cloning",
            "replicate",
            "replicating",
            "replication",
        ] {
            assert!(
                !contains_prompt_term(prompt, blocked),
                "prompt contained blocked term `{blocked}`: {prompt}"
            );
        }
    }
}
