#!/usr/bin/env python3
"""Standard-library audio worker for the first Timbreprint MVP flow."""

from __future__ import annotations

import json
import math
import statistics
import struct
import sys
import wave
from pathlib import Path

FRAME_SIZE = 2048


def scored(value: str | int, confidence: float) -> dict[str, str | int | float]:
    return {"value": value, "confidence": round(confidence, 2)}


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


def analyze_wav(path: Path) -> dict[str, object]:
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

    return {
        "tempo": scored(tempo, tempo_confidence),
        "key": scored("unknown", 0.1),
        "energy": scored(energy, energy_confidence),
        "mood": mood,
        "genre": genre,
        "instruments": instruments,
        "texture": texture,
        "features": {
            "durationSeconds": round(duration, 2),
            "rms": round(rms, 4),
            "zeroCrossingRate": round(zero_cross_rate, 4),
        },
    }


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


def classify_texture(zero_cross_rate: float, rms: float) -> list[dict[str, str | float]]:
    if zero_cross_rate < 0.04:
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


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: audio_analysis.py <job-dir>", file=sys.stderr)
        return 2

    job_dir = Path(sys.argv[1])
    processed_path = job_dir / "processed.wav"
    if not processed_path.exists():
        print(f"missing processed audio: {processed_path}", file=sys.stderr)
        return 1

    try:
        analysis = analyze_wav(processed_path)
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
