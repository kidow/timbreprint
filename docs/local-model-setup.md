# Local Model Setup

Timbreprint works without optional models. When local models are available, it
uses them as quality upgrades and falls back to the template/heuristic path when
they are missing.

## Essentia Tagging

Essentia tagging is optional. It improves `genre`, `mood`, `instruments`, and
voice/instrumental texture tags.

Install the Python dependency in the worker environment:

```bash
workers/.venv/bin/python -m pip install essentia
```

Place model files under the app data directory:

```text
~/Library/Application Support/com.kidow.timbreprint/models/essentia/
```

Required files:

```text
discogs_label_embeddings-effnet-bs64-1.pb
mtg_jamendo_genre-discogs_label_embeddings-effnet-1.pb
mtg_jamendo_genre-discogs_label_embeddings-effnet-1.json
mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.pb
mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.json
mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.pb
mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.json
voice_instrumental-discogs-effnet-1.pb
voice_instrumental-discogs-effnet-1.json
```

After a run, check:

- UI `Tagging` status: `Essentia`
- `analysis.json`: `features.analysisBackend` includes `essentia-effnet`
- `analysis.json`: `features.taggingModelStatus.available` is `true`

If Essentia is missing or model files are missing, Timbreprint keeps using the
local heuristic tags.

## Ollama Prompt Rewrite

Ollama rewrite is optional. It rewrites the deterministic template prompt into
polished English and then applies the sanitizer again.

Start Ollama and pull a small local model:

```bash
ollama pull qwen2.5:3b
```

Use a different model if needed:

```bash
TIMBREPRINT_OLLAMA_MODEL=llama3.2:3b pnpm tauri dev
```

After a run, check:

- UI `Prompt rewrite` status: `Ollama`
- `prompt-rewrite.json`: `used` is `true`
- `prompt-template.txt`: deterministic fallback prompt
- `prompt.txt`: final prompt shown in the app

If Ollama is not running, the model is missing, or the request fails,
Timbreprint keeps the deterministic template prompt.

## Local Verification

Run the non-model tests:

```bash
python3 -m unittest workers.test_audio_analysis
pnpm build
cd src-tauri
cargo test
```

Run a real audio file through the desktop app:

```bash
pnpm tauri dev
```

Then inspect the generated job directory from the `Open output` button.
