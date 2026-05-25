# Audio Tagging Model Review

Date: 2026-05-25

This note compares local audio tagging options for Timbreprint. The goal is to
improve `genre`, `mood`, `instruments`, and related prompt descriptors while
keeping the app local-first and practical on Apple Silicon.

## Recommendation

Start with Essentia's TensorFlow models, using the Discogs EffNet embedding
model plus MTG-Jamendo classifiers.

Recommended first integration:

1. `discogs_label_embeddings-effnet-bs64-1.pb`
2. `mtg_jamendo_genre-discogs_label_embeddings-effnet-1.pb`
3. `mtg_jamendo_moodtheme-discogs_label_embeddings-effnet-1.pb`
4. `mtg_jamendo_instrument-discogs_label_embeddings-effnet-1.pb`
5. `voice_instrumental-discogs-effnet-1.pb`

This path maps directly to the current JSON contract because it produces
classifier probabilities that can become `ScoredValue` confidence scores. It is
also narrower than general embedding models, so the first integration can stay
inside the existing Python worker fallback structure.

## Candidate Comparison

| Candidate | Best use | Fit for MVP | Local complexity | Notes |
| --- | --- | --- | --- | --- |
| Essentia + MTG-Jamendo classifiers | Genre, mood/theme, instruments, voice/instrumental | High | Medium | Best first target. Official examples expose direct classifier outputs and metadata files. |
| CLAP / LAION-CLAP | Text-audio similarity, open vocabulary matching | Medium | Medium-high | Useful later for custom prompt descriptors, but needs curated text labels and threshold tuning. |
| MERT-v1-95M | General music embeddings and downstream classifiers | Medium-low for MVP | High | Strong representation model, but not a direct tagger unless paired with a classifier head or label pipeline. |
| Essentia single classifiers | Danceability, moods, voice/instrumental | Medium | Medium | Useful as add-ons after the MTG-Jamendo flow works. |

## Why Essentia First

Essentia provides Python examples for loading audio at 16 kHz, extracting
Discogs EffNet embeddings, and passing those embeddings to MTG-Jamendo genre,
mood/theme, instrument, and top-tag classifiers. That fits the current
`workers/audio_analysis.py` shape: load `processed.wav`, derive features, return
JSON.

The output can be merged conservatively:

- Keep current heuristic tags as fallback.
- If model inference succeeds, replace or append model tags.
- Store model tags with numeric confidence from classifier probabilities.
- Record `features.analysisBackend` as something like `librosa+essentia-effnet`.

## Deferred Options

CLAP should wait until after the first supervised tagging path works. It is
better for matching audio against custom text prompts such as "warm analog synth"
or "wide cinematic atmosphere", but those labels need a curated vocabulary and
calibration.

MERT should also wait. The 95M model is suitable for embeddings and downstream
tasks, but integrating it cleanly requires PyTorch/Transformers dependencies,
pooling decisions, and a classifier or retrieval layer. That is more risk than
needed for the next MVP increment.

## Next Integration Plan

1. Add an optional `essentia` dependency path that does not break the existing
   `librosa`/stdlib fallback.
2. Add a local model directory under the app data directory, not committed to
   Git.
3. Implement a small model availability check in the Python worker.
4. Run genre, mood/theme, instrument, and voice/instrumental classifiers when
   model files exist.
5. Merge classifier results into `analysis.json` with confidence scores.
6. Add tests around model-output normalization using fixture JSON, without
   requiring model downloads during unit tests.

## Sources

- Essentia model catalog: https://essentia.upf.edu/models.html
- Essentia TensorFlow auto-tagging tutorial: https://essentia.upf.edu/tutorial_tensorflow_auto-tagging_classification_embeddings.html
- LAION-CLAP PyPI package: https://pypi.org/project/laion-clap/
- MERT-v1-95M Hugging Face model card: https://huggingface.co/m-a-p/MERT-v1-95M
