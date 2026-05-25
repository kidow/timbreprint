from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import audio_analysis


class AudioTaggingNormalizationTests(unittest.TestCase):
    def test_load_essentia_labels_normalizes_metadata_classes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            metadata_path = Path(directory) / "model.json"
            metadata_path.write_text(
                json.dumps(
                    {
                        "classes": [
                            "genre---Electronic",
                            "mood/theme---Warm_Dream",
                            "instrument---Electric-Guitar",
                        ]
                    }
                ),
                encoding="utf-8",
            )

            labels = audio_analysis.load_essentia_labels(metadata_path)

        self.assertEqual(labels, ["electronic", "warm dream", "electric guitar"])

    def test_top_scored_labels_filters_and_sorts_confidence(self) -> None:
        labels = ["ambient", "rock", "jazz", "noise"]
        scores = [0.2, 0.08, 0.7, 0.3]

        tagged = audio_analysis.top_scored_labels(labels, scores, limit=2)

        self.assertEqual(
            tagged,
            [
                {"value": "jazz", "confidence": 0.7},
                {"value": "noise", "confidence": 0.3},
            ],
        )

    def test_model_status_requires_metadata_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            model_dir = Path(directory) / "essentia"
            model_dir.mkdir()
            for filename in audio_analysis.ESSENTIA_MODEL_FILES.values():
                if filename.endswith(".pb"):
                    (model_dir / filename).write_text("", encoding="utf-8")

            status = audio_analysis.essentia_model_status(Path(directory))

        self.assertFalse(status["available"])
        self.assertIn("mtg_jamendo_genre-discogs_label_embeddings-effnet-1.json", status["missing"])


if __name__ == "__main__":
    unittest.main()
