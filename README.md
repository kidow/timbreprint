# Timbreprint

Timbreprint is a local-first desktop app for turning a reference music file into a structured musical analysis and an English prompt for music generation tools.

The first MVP focuses on the analysis and prompt workflow, not on generating new music directly. It accepts common audio files, creates a local job folder, writes analysis JSON with confidence scores, and produces a prompt that describes abstract musical traits instead of copying an artist or song.

## What It Does

- Opens a Tauri desktop shell with a React and TypeScript UI.
- Lets you select `mp3`, `wav`, `m4a`, or `flac` files.
- Keeps the MVP input scope to files up to 10 minutes.
- Creates a local job directory for each analysis run.
- Writes `job.json`, `source-metadata.json`, `analysis.json`, `prompt.txt`, and `processed.wav`.
- Shows tempo, key, energy, genre, mood, instruments, texture, and confidence labels.
- Generates an English prompt from a deterministic template.

## Current Status

This repo is in the first scaffolded MVP stage.

Implemented:

- Tauri 2 app scaffold
- Vite + React + TypeScript frontend
- Single-screen MVP UI
- Environment status panel
- Browser preview fallback flow
- Rust commands for local job creation
- Real `ffmpeg` conversion to `processed.wav`
- Python worker analysis using standard-library WAV feature extraction
- JSON and prompt file output

Not implemented yet:

- `librosa` analysis
- Audio tagging models
- Ollama or local LLM prompt rewriting
- MusicGen or other local music generation
- App packaging and signing

## Requirements

- macOS
- Node.js and pnpm
- Rust and Cargo
- Tauri system prerequisites

Optional for later MVP steps:

- `ffmpeg`
- `python3`
- Python packages in `workers/requirements.txt` for better audio analysis

## Install

```bash
pnpm install
```

Optional Python analysis dependencies:

```bash
python3 -m venv workers/.venv
workers/.venv/bin/python -m pip install -r workers/requirements.txt
```

The Tauri worker automatically prefers `workers/.venv/bin/python` when it exists. If the local virtual environment is missing, it falls back to `python3` or `python` from `PATH`.

If you intentionally want to install into your active Python environment instead:

```bash
python3 -m pip install -r workers/requirements.txt
```

## Start The Web Preview

Use this when you want to inspect the React UI in a browser:

```bash
pnpm dev
```

Then open:

```text
http://localhost:1420
```

In browser preview mode, native Tauri commands are not available. The UI uses a static fallback so you can still click through the basic flow.

## Start The Desktop App

Use this when you want to run the actual Tauri app:

```bash
pnpm tauri dev
```

The desktop app can call the Rust commands that create local job output.

## Build

Build the frontend:

```bash
pnpm build
```

Check the Rust/Tauri side:

```bash
cd src-tauri
cargo check
```

## Local Job Output

The app stores job data under the platform app data directory. On macOS, that is typically:

```text
~/Library/Application Support/com.kidow.timbreprint/jobs/
```

Each job is expected to look like this:

```text
jobs/
  2026-05-25_143000_track-name/
    job.json
    source-metadata.json
    processed.wav
    analysis.json
    prompt.txt
```

## Development Notes

- The current analysis uses real WAV samples for basic tempo, energy, and texture heuristics.
- If `librosa` and `numpy` are installed, the worker uses them for stronger tempo, spectral, onset, and tonal-center analysis.
- The worker checks the local app data `models/essentia` directory for optional Essentia tagging models, records availability in `features.taggingModelStatus`, and uses them for genre, mood, instrument, and voice/instrumental tags when all files are present.
- Key, genre, mood, and instrument labels are still heuristic and low-confidence until model-based tagging is added.
- Confidence is stored as a `0-1` number in JSON and shown as low, medium, or high in the UI.
- The generated prompt is English-only for now.
- Prompt generation avoids direct artist, song, copy, clone, and replication language.
- `librosa` or model-based audio analysis should be added after the file/job/prompt contract stays stable.
