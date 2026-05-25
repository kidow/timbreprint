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
    fs::create_dir_all(&logs_dir)?;

    Ok(ToolStatus {
        ffmpeg: find_executable("ffmpeg"),
        python: worker_python_path().or_else(|| find_executable("python3").or_else(|| find_executable("python"))),
        app_data_dir: app_data_dir.display().to_string(),
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
    fs::create_dir_all(&jobs_dir)?;

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

    run_python_worker(&job_dir)?;
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

fn run_python_worker(job_dir: &Path) -> Result<(), AppError> {
    let python = worker_python_path()
        .or_else(|| find_executable("python3").or_else(|| find_executable("python")))
        .ok_or(AppError::MissingPython)?;
    let worker_path = worker_script_path()?;

    let output = Command::new(python)
        .arg(worker_path)
        .arg(job_dir)
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
    let genre = join_values(&analysis.genre);
    let mood = join_values(&analysis.mood);
    let instruments = join_values(&analysis.instruments);
    let texture = join_values(&analysis.texture);

    format!(
        "Create a {tempo} BPM {genre} track in {key} with {instruments}. The mood should feel {mood}, with {texture} texture and {energy} energy.",
        tempo = analysis.tempo.value,
        genre = genre,
        key = analysis.key.value,
        instruments = instruments,
        mood = mood,
        texture = texture,
        energy = analysis.energy.value
    )
}

fn join_values(items: &[ScoredValue<String>]) -> String {
    items
        .iter()
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
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

        run_python_worker(&test_dir).expect("run python worker");
        let analysis = read_analysis(&test_dir).expect("read analysis");

        assert!(matches!(
            analysis.energy.value.as_str(),
            "low" | "medium" | "high"
        ));
        assert!(!analysis.texture.is_empty());
        assert!(test_dir.join("worker.log").exists());

        let _ = fs::remove_dir_all(test_dir);
    }
}
