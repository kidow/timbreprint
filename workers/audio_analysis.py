#!/usr/bin/env python3
"""Audio analysis worker for the first Timbreprint MVP flow."""

from __future__ import annotations

import importlib.util
import json
import math
import statistics
import struct
import sys
import wave
from pathlib import Path

FRAME_SIZE = 2048
ESSENTIA_MODEL_FILES = {
    "embedding": "discogs_label_embeddings-effnet-bs64-1.pb",
    "genre": "mtg_jamendo_genre-discogs_label_embeddings-effnet-1.pb",
    "genreMetadata": "mtg_jamendo_genre-discogs_label_embeddings-effnet-1.json",
    "moodTheme": "mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.pb",
    "moodThemeMetadata": "mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.json",
    "instrument": "mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.pb",
    "instrumentMetadata": "mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.json",
    "voiceInstrumental": "voice_instrumental-discogs-effnet-1.pb",
    "voiceInstrumentalMetadata": "voice_instrumental-discogs-effnet-1.json",
}

ESSENTIA_CLASSIFIERS = {
    "genre": (
        "mtg_jamendo_genre-discogs_label_embeddings-effnet-1.pb",
        "mtg_jamendo_genre-discogs_label_embeddings-effnet-1.json",
    ),
    "mood": (
        "mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.pb",
        "mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.json",
    ),
    "instruments": (
        "mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.pb",
        "mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.json",
    ),
    "voice": (
        "voice_instrumental-discogs-effnet-1.pb",
        "voice_instrumental-discogs-effnet-1.json",
    ),
}


def scored(value: str | int | float, confidence: float) -> dict[str, str | int | float]:
    return {"value": value, "confidence": round(confidence, 2)}


def optional_librosa_available() -> bool:
    return bool(importlib.util.find_spec("librosa") and importlib.util.find_spec("numpy"))


def optional_essentia_available() -> bool:
    return bool(importlib.util.find_spec("essentia"))


def essentia_model_status(models_dir: Path | None) -> dict[str, object]:
    if models_dir is None:
        return {
            "available": False,
            "libraryAvailable": optional_essentia_available(),
            "modelsDir": None,
            "missing": list(ESSENTIA_MODEL_FILES.values()),
        }

    missing = [
        filename
        for filename in ESSENTIA_MODEL_FILES.values()
        if not (models_dir / "essentia" / filename).is_file()
    ]
    return {
        "available": optional_essentia_available() and not missing,
        "libraryAvailable": optional_essentia_available(),
        "modelsDir": str(models_dir / "essentia"),
        "missing": missing,
    }


def merge_model_tags(analysis: dict[str, object], path: Path, models_dir: Path | None) -> dict[str, object]:
    status = essentia_model_status(models_dir)
    features = analysis.setdefault("features", {})
    if isinstance(features, dict):
        features["taggingModelStatus"] = status

    if not status["available"] or models_dir is None:
        return analysis

    try:
        model_tags = run_essentia_tagging(path, models_dir / "essentia")
    except Exception as exc:
        status["available"] = False
        status["error"] = str(exc)
        return analysis

    if model_tags.get("genre"):
        analysis["genre"] = model_tags["genre"]
    if model_tags.get("mood"):
        analysis["mood"] = model_tags["mood"]
    if model_tags.get("instruments"):
        analysis["instruments"] = model_tags["instruments"]
    if model_tags.get("voice"):
        analysis["texture"] = merge_scored_values(
            analysis.get("texture", []),
            model_tags["voice"],
            limit=4,
        )

    if isinstance(features, dict):
        features["analysisBackend"] = f"{features.get('analysisBackend', 'audio')}+essentia-effnet"
    return analysis


def run_essentia_tagging(path: Path, model_dir: Path) -> dict[str, list[dict[str, str | float]]]:
    from essentia.standard import MonoLoader, TensorflowPredict2D, TensorflowPredictEffnetDiscogs
    import numpy as np

    audio = MonoLoader(filename=str(path), sampleRate=16000, resampleQuality=4)()
    embedding_model = TensorflowPredictEffnetDiscogs(
        graphFilename=str(model_dir / ESSENTIA_MODEL_FILES["embedding"]),
        output="PartitionedCall:1",
    )
    embeddings = embedding_model(audio)

    output: dict[str, list[dict[str, str | float]]] = {}
    for target, (model_name, metadata_name) in ESSENTIA_CLASSIFIERS.items():
        model = TensorflowPredict2D(graphFilename=str(model_dir / model_name))
        predictions = model(embeddings)
        scores = np.mean(np.asarray(predictions, dtype=float), axis=0).tolist()
        labels = load_essentia_labels(model_dir / metadata_name)
        output[target] = top_scored_labels(labels, scores)

    return output


def load_essentia_labels(path: Path) -> list[str]:
    metadata = json.loads(path.read_text(encoding="utf-8"))
    labels = (
        metadata.get("classes")
        or metadata.get("class_names")
        or metadata.get("tags")
        or metadata.get("labels")
    )
    if isinstance(labels, dict):
        labels = list(labels.values())
    if not isinstance(labels, list):
        raise ValueError(f"metadata has no class labels: {path}")
    return [normalize_essentia_label(str(label)) for label in labels]


def normalize_essentia_label(label: str) -> str:
    value = label.strip().lower()
    if "---" in value:
        value = value.split("---", 1)[1]
    value = value.replace("_", " ").replace("-", " ")
    return " ".join(value.split())


def top_scored_labels(
    labels: list[str],
    scores: list[float],
    limit: int = 4,
    minimum_confidence: float = 0.12,
) -> list[dict[str, str | float]]:
    paired = [
        (label, float(score))
        for label, score in zip(labels, scores)
        if label and float(score) >= minimum_confidence
    ]
    paired.sort(key=lambda item: item[1], reverse=True)
    return [scored(label, confidence) for label, confidence in paired[:limit]]


def merge_scored_values(
    existing: object,
    incoming: list[dict[str, str | float]],
    limit: int,
) -> list[dict[str, str | float]]:
    merged: dict[str, dict[str, str | float]] = {}
    if isinstance(existing, list):
        for item in existing:
            if isinstance(item, dict) and isinstance(item.get("value"), str):
                merged[str(item["value"])] = item
    for item in incoming:
        if isinstance(item.get("value"), str):
            merged[str(item["value"])] = item
    return list(merged.values())[:limit]


def analyze_with_librosa(path: Path, models_dir: Path | None = None) -> dict[str, object] | None:
    if not optional_librosa_available():
        return None

    import librosa
    import numpy as np

    y, sample_rate = librosa.load(path, sr=44100, mono=True)
    if y.size == 0:
        raise ValueError("processed WAV has no samples")

    duration = float(librosa.get_duration(y=y, sr=sample_rate))
    tempo_raw, _beats = librosa.beat.beat_track(y=y, sr=sample_rate)
    tempo = int(round(float(np.asarray(tempo_raw).reshape(-1)[0]))) if np.size(tempo_raw) else 0
    rms = float(np.mean(librosa.feature.rms(y=y)[0]))
    zero_cross_rate = float(np.mean(librosa.feature.zero_crossing_rate(y)[0]))
    spectral_centroid = float(np.mean(librosa.feature.spectral_centroid(y=y, sr=sample_rate)[0]))
    onset_env = librosa.onset.onset_strength(y=y, sr=sample_rate)
    onset_density = float(np.mean(onset_env)) if onset_env.size else 0.0
    key, key_confidence = estimate_key(y, sample_rate, librosa, np)

    energy, energy_confidence = classify_energy(rms)
    texture = classify_texture(zero_cross_rate, rms, spectral_centroid)
    mood = classify_mood(energy, texture[0]["value"], tempo)
    genre = classify_genre(energy, texture[0]["value"], tempo)
    instruments = classify_instruments(texture[0]["value"], energy)
    brightness = classify_brightness(spectral_centroid)
    rhythm = classify_rhythm(onset_density, tempo)
    dynamics = classify_dynamics(rms)
    space = classify_space(rms, texture[0]["value"])
    arrangement = classify_arrangement(energy, rhythm[0]["value"], duration)
    negative_prompt = classify_negative_prompt(brightness[0]["value"], energy)

    backend = "librosa"
    model_status = essentia_model_status(models_dir)
    if model_status["available"]:
        backend = "librosa+essentia-ready"

    analysis = {
        "tempo": scored(tempo, 0.7 if tempo > 0 else 0.15),
        "key": scored(key, key_confidence),
        "energy": scored(energy, energy_confidence),
        "mood": mood,
        "genre": genre,
        "instruments": instruments,
        "texture": texture,
        "rhythm": rhythm,
        "dynamics": dynamics,
        "brightness": brightness,
        "space": space,
        "arrangement": arrangement,
        "negativePrompt": negative_prompt,
        "features": {
            "analysisBackend": backend,
            "durationSeconds": round(duration, 2),
            "rms": round(rms, 4),
            "zeroCrossingRate": round(zero_cross_rate, 4),
            "spectralCentroidHz": round(spectral_centroid, 2),
            "onsetDensity": round(onset_density, 4),
            "taggingModelStatus": model_status,
        },
    }
    return merge_model_tags(analysis, path, models_dir)


def estimate_key(y, sample_rate: int, librosa, np) -> tuple[str, float]:
    chroma = librosa.feature.chroma_cqt(y=y, sr=sample_rate)
    if chroma.size == 0:
        return "unknown", 0.1

    pitch_profile = np.mean(chroma, axis=1)
    pitch_index = int(np.argmax(pitch_profile))
    pitch_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
    total = float(np.sum(pitch_profile))
    confidence = 0.15 if total == 0 else min(0.55, float(pitch_profile[pitch_index] / total) * 3)
    return f"{pitch_names[pitch_index]} tonal center", confidence


def iter_mono_chunks(path: Path):
    with wave.open(str(path), "rb") as wav:
        channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        sample_rate = wav.getframerate()
        total_frames = wav.getnframes()

        if sample_width != 2:
            raise ValueError(f"unsupported WAV sample width: {sample_width}")

        while True:
            raw = wav.readframes(FRAME_SIZE)
            if not raw:
                break

            values = struct.unpack("<" + "h" * (len(raw) // sample_width), raw)
            mono = []
            for index in range(0, len(values), channels):
                frame = values[index : index + channels]
                mono.append(sum(frame) / (channels * 32768.0))

            yield mono, sample_rate, total_frames


def analyze_wav(path: Path, models_dir: Path | None = None) -> dict[str, object]:
    sample_rate = 44100
    total_frames = 0
    total_samples = 0
    sum_squares = 0.0
    zero_crossings = 0
    previous = 0.0
    chunk_energies: list[float] = []

    for chunk, sample_rate, total_frames in iter_mono_chunks(path):
        if not chunk:
            continue

        total_samples += len(chunk)
        chunk_sum_squares = sum(sample * sample for sample in chunk)
        sum_squares += chunk_sum_squares
        chunk_energies.append(math.sqrt(chunk_sum_squares / len(chunk)))

        for sample in chunk:
            if (previous < 0 <= sample) or (previous >= 0 > sample):
                zero_crossings += 1
            previous = sample

    if total_samples == 0:
        raise ValueError("processed WAV has no samples")

    duration = total_frames / sample_rate
    rms = math.sqrt(sum_squares / total_samples)
    zero_cross_rate = zero_crossings / total_samples
    tempo, tempo_confidence = estimate_tempo(chunk_energies, sample_rate)
    energy, energy_confidence = classify_energy(rms)
    texture = classify_texture(zero_cross_rate, rms)
    mood = classify_mood(energy, texture[0]["value"], tempo)
    genre = classify_genre(energy, texture[0]["value"], tempo)
    instruments = classify_instruments(texture[0]["value"], energy)
    brightness = classify_brightness(None, zero_cross_rate)
    rhythm = classify_rhythm(None, tempo)
    dynamics = classify_dynamics(rms)
    space = classify_space(rms, texture[0]["value"])
    arrangement = classify_arrangement(energy, rhythm[0]["value"], duration)
    negative_prompt = classify_negative_prompt(brightness[0]["value"], energy)

    analysis = {
        "tempo": scored(tempo, tempo_confidence),
        "key": scored("unknown", 0.1),
        "energy": scored(energy, energy_confidence),
        "mood": mood,
        "genre": genre,
        "instruments": instruments,
        "texture": texture,
        "rhythm": rhythm,
        "dynamics": dynamics,
        "brightness": brightness,
        "space": space,
        "arrangement": arrangement,
        "negativePrompt": negative_prompt,
        "features": {
            "analysisBackend": "stdlib-wave",
            "durationSeconds": round(duration, 2),
            "rms": round(rms, 4),
            "zeroCrossingRate": round(zero_cross_rate, 4),
            "taggingModelStatus": essentia_model_status(models_dir),
        },
    }
    return merge_model_tags(analysis, path, models_dir)


def estimate_tempo(energies: list[float], sample_rate: int) -> tuple[int, float]:
    if len(energies) < 4:
        return 0, 0.1

    mean = statistics.fmean(energies)
    deviation = statistics.pstdev(energies) if len(energies) > 1 else 0.0
    threshold = mean + deviation * 0.8
    seconds_per_frame = FRAME_SIZE / sample_rate
    peaks: list[int] = []

    for index in range(1, len(energies) - 1):
        if (
            energies[index] > threshold
            and energies[index] >= energies[index - 1]
            and energies[index] >= energies[index + 1]
        ):
            if not peaks or (index - peaks[-1]) * seconds_per_frame >= 0.2:
                peaks.append(index)

    intervals = [
        (right - left) * seconds_per_frame
        for left, right in zip(peaks, peaks[1:])
        if 0.25 <= (right - left) * seconds_per_frame <= 1.5
    ]

    if not intervals:
        return 0, 0.15

    interval = statistics.median(intervals)
    tempo = round(60 / interval)
    while tempo < 60:
        tempo *= 2
    while tempo > 180:
        tempo = round(tempo / 2)

    confidence = min(0.75, 0.35 + len(intervals) / 20)
    return int(tempo), confidence


def classify_energy(rms: float) -> tuple[str, float]:
    if rms < 0.035:
        return "low", 0.65
    if rms < 0.11:
        return "medium", 0.7
    return "high", 0.75


def classify_texture(
    zero_cross_rate: float,
    rms: float,
    spectral_centroid: float | None = None,
) -> list[dict[str, str | float]]:
    if spectral_centroid is not None and spectral_centroid > 3200:
        primary = "bright"
    elif spectral_centroid is not None and spectral_centroid < 1400:
        primary = "smooth"
    elif zero_cross_rate < 0.04:
        primary = "smooth"
    elif zero_cross_rate < 0.1:
        primary = "balanced"
    else:
        primary = "bright"

    density = "sparse" if rms < 0.035 else "full" if rms > 0.11 else "moderate"
    return [scored(primary, 0.55), scored(density, 0.5)]


def classify_mood(energy: str, texture: str, tempo: int) -> list[dict[str, str | float]]:
    if energy == "high" and tempo >= 115:
        return [scored("driving", 0.5), scored("energetic", 0.5)]
    if energy == "low":
        return [scored("calm", 0.55), scored("intimate", 0.45)]
    if texture == "bright":
        return [scored("clear", 0.45), scored("focused", 0.4)]
    return [scored("steady", 0.45), scored("warm", 0.4)]


def classify_genre(energy: str, texture: str, tempo: int) -> list[dict[str, str | float]]:
    if energy == "high" and tempo >= 115:
        return [scored("electronic", 0.35), scored("dance", 0.3)]
    if texture == "smooth" and energy == "low":
        return [scored("ambient", 0.35), scored("cinematic", 0.25)]
    return [scored("instrumental", 0.25), scored("experimental", 0.2)]


def classify_instruments(texture: str, energy: str) -> list[dict[str, str | float]]:
    if texture == "bright":
        return [scored("percussive elements", 0.25), scored("bright lead", 0.2)]
    if energy == "low":
        return [scored("soft pads", 0.25), scored("minimal percussion", 0.2)]
    return [scored("synth layers", 0.25), scored("steady drums", 0.2)]


def classify_brightness(
    spectral_centroid: float | None,
    zero_cross_rate: float | None = None,
) -> list[dict[str, str | float]]:
    if spectral_centroid is not None:
        if spectral_centroid >= 3200:
            return [scored("bright upper range", 0.62), scored("crisp tone", 0.5)]
        if spectral_centroid <= 1400:
            return [scored("dark low-mid focus", 0.62), scored("soft highs", 0.5)]
        return [scored("balanced frequency profile", 0.56)]

    if zero_cross_rate is not None and zero_cross_rate > 0.1:
        return [scored("bright upper range", 0.42)]
    if zero_cross_rate is not None and zero_cross_rate < 0.04:
        return [scored("dark low-mid focus", 0.42)]
    return [scored("balanced frequency profile", 0.38)]


def classify_rhythm(
    onset_density: float | None,
    tempo: int,
) -> list[dict[str, str | float]]:
    if onset_density is not None and onset_density >= 0.25:
        return [scored("active rhythmic motion", 0.62), scored("clearly marked pulse", 0.52)]
    if onset_density is not None and 0 < onset_density < 0.08:
        return [scored("spacious rhythm", 0.58), scored("minimal attack pattern", 0.46)]
    if tempo >= 115:
        return [scored("steady forward pulse", 0.45)]
    if tempo > 0 and tempo <= 85:
        return [scored("slow measured pulse", 0.45)]
    return [scored("moderate steady pulse", 0.38)]


def classify_dynamics(rms: float) -> list[dict[str, str | float]]:
    if rms < 0.035:
        return [scored("restrained dynamics", 0.64), scored("soft transients", 0.48)]
    if rms > 0.11:
        return [scored("bold dynamics", 0.68), scored("forward impact", 0.52)]
    return [scored("controlled dynamics", 0.58), scored("even loudness", 0.44)]


def classify_space(rms: float, texture: str) -> list[dict[str, str | float]]:
    if rms < 0.035 or texture == "smooth":
        return [scored("wide atmospheric space", 0.55), scored("gentle reverb tail", 0.45)]
    if texture == "bright":
        return [scored("clear foreground presence", 0.48)]
    return [scored("balanced stereo space", 0.44)]


def classify_arrangement(
    energy: str,
    rhythm: str | int | float,
    duration: float,
) -> list[dict[str, str | float]]:
    if energy == "high":
        return [
            scored("open with a short focused intro", 0.44),
            scored("build toward a fuller central section", 0.48),
        ]
    if "spacious" in str(rhythm) or duration >= 180:
        return [
            scored("start minimal and let layers enter gradually", 0.46),
            scored("keep transitions smooth and unhurried", 0.44),
        ]
    return [
        scored("establish the main groove early", 0.4),
        scored("add subtle variation between sections", 0.38),
    ]


def classify_negative_prompt(
    brightness: str | int | float,
    energy: str,
) -> list[dict[str, str | float]]:
    items = [
        scored("artist-specific references", 0.9),
        scored("recreating existing songs", 0.9),
        scored("recognizable copyrighted melody", 0.9),
    ]
    if "bright" in str(brightness):
        items.append(scored("harsh treble", 0.5))
    if energy == "low":
        items.append(scored("overly compressed drums", 0.45))
    return items


def main() -> int:
    if len(sys.argv) not in {2, 3}:
        print("usage: audio_analysis.py <job-dir> [models-dir]", file=sys.stderr)
        return 2

    job_dir = Path(sys.argv[1])
    models_dir = Path(sys.argv[2]) if len(sys.argv) == 3 else None
    processed_path = job_dir / "processed.wav"
    if not processed_path.exists():
        print(f"missing processed audio: {processed_path}", file=sys.stderr)
        return 1

    try:
        analysis = analyze_with_librosa(processed_path, models_dir) or analyze_wav(
            processed_path,
            models_dir,
        )
    except Exception as exc:
        print(f"failed to analyze audio: {exc}", file=sys.stderr)
        return 1

    analysis_path = job_dir / "analysis.json"
    analysis_path.write_text(
        json.dumps(analysis, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"status": "ok", "analysisPath": str(analysis_path)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
