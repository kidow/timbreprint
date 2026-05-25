use chrono::Local;
use serde::Serialize;
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

#[derive(Serialize)]
struct ScoredValue<T> {
    value: T,
    confidence: f32,
}

#[derive(Serialize)]
struct Analysis {
    tempo: ScoredValue<u16>,
    key: ScoredValue<&'static str>,
    energy: ScoredValue<&'static str>,
    mood: Vec<ScoredValue<&'static str>>,
    genre: Vec<ScoredValue<&'static str>>,
    instruments: Vec<ScoredValue<&'static str>>,
    texture: Vec<ScoredValue<&'static str>>,
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
        python: find_executable("python3").or_else(|| find_executable("python")),
        app_data_dir: app_data_dir.display().to_string(),
        logs_dir: logs_dir.display().to_string(),
    })
}

#[tauri::command]
fn run_mock_analysis(app: AppHandle, source_path: String) -> Result<JobResult, AppError> {
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
    write_placeholder_wav(&processed_path)?;

    let analysis = mock_analysis();
    let prompt = generate_prompt(&analysis);

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

    let job_state = JobState {
        id: &id,
        status: "completed",
        source_path: &source_path,
        processed_path: processed_path.display().to_string(),
        created_at: Local::now().to_rfc3339(),
    };

    write_json(job_dir.join("source-metadata.json"), &source_metadata)?;
    write_json(job_dir.join("analysis.json"), &analysis)?;
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
            run_mock_analysis,
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
    value.trim().parse::<f64>().ok().map(|seconds| seconds.ceil() as u64)
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

fn mock_analysis() -> Analysis {
    Analysis {
        tempo: ScoredValue {
            value: 92,
            confidence: 0.8,
        },
        key: ScoredValue {
            value: "A minor",
            confidence: 0.6,
        },
        energy: ScoredValue {
            value: "medium",
            confidence: 0.7,
        },
        mood: vec![
            ScoredValue {
                value: "melancholic",
                confidence: 0.8,
            },
            ScoredValue {
                value: "warm",
                confidence: 0.7,
            },
            ScoredValue {
                value: "hopeful",
                confidence: 0.6,
            },
        ],
        genre: vec![
            ScoredValue {
                value: "indie electronic",
                confidence: 0.7,
            },
            ScoredValue {
                value: "ambient pop",
                confidence: 0.6,
            },
        ],
        instruments: vec![
            ScoredValue {
                value: "analog synth",
                confidence: 0.6,
            },
            ScoredValue {
                value: "soft drums",
                confidence: 0.6,
            },
            ScoredValue {
                value: "bass pad",
                confidence: 0.5,
            },
        ],
        texture: vec![
            ScoredValue {
                value: "spacious",
                confidence: 0.7,
            },
            ScoredValue {
                value: "reverb-heavy",
                confidence: 0.7,
            },
            ScoredValue {
                value: "intimate",
                confidence: 0.6,
            },
        ],
    }
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

fn join_values(items: &[ScoredValue<&str>]) -> String {
    items
        .iter()
        .map(|item| item.value)
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<(), AppError> {
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{body}\n"))?;
    Ok(())
}

fn write_placeholder_wav(path: &Path) -> Result<(), AppError> {
    fs::write(path, b"placeholder wav: ffmpeg integration follows\n")?;
    Ok(())
}
