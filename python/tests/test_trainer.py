from __future__ import annotations

import importlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_trainer = importlib.import_module("neurohid_ml.trainer")


def _session_payload(sample_count: int, feature_dim: int) -> dict:
    episodes = []
    for i in range(sample_count):
        episodes.append(
            {
                "feature_values": [float(i + j) for j in range(feature_dim)],
                "decoder_confidence": 0.6,
                "action": {
                    "mouse": {
                        "movement": {"dx": float(i % 3), "dy": float((i + 1) % 3)},
                        "buttons": [{"button": "Left", "pressed": i % 2 == 0}],
                    },
                    "confidence": 0.5,
                },
            }
        )
    return {"episodes": episodes}


class TrainerTests(unittest.TestCase):
    def test_train_candidate_model_writes_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            session_log = root / "session_001.json"
            session_log.write_text(
                json.dumps(_session_payload(sample_count=8, feature_dim=6)),
                encoding="utf-8",
            )

            output_dir = root / "candidate"
            def fake_export(*args, **kwargs) -> None:
                onnx_path = args[2]
                Path(onnx_path).write_bytes(b"fake-onnx")

            with patch.object(_trainer.torch.onnx, "export", side_effect=fake_export):
                result = _trainer.train_candidate_model(
                    session_logs=[session_log],
                    output_dir=output_dir,
                    model_version="candidate-test",
                    config=_trainer.TrainerConfig(
                        epochs=1,
                        learning_rate=1e-3,
                        holdout_ratio=0.25,
                        seed=7,
                        decode_latency_p95_us=40_000,
                        min_samples=4,
                    ),
                )

            self.assertTrue(result.onnx_path.exists())
            self.assertTrue(result.manifest_path.exists())
            self.assertTrue(result.metrics_path.exists())
            self.assertGreaterEqual(result.holdout_sample_count, 1)

    def test_train_candidate_model_rejects_small_dataset(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            session_log = root / "session_002.json"
            session_log.write_text(
                json.dumps(_session_payload(sample_count=2, feature_dim=4)),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "insufficient samples"):
                _trainer.train_candidate_model(
                    session_logs=[session_log],
                    output_dir=root / "candidate",
                    model_version="candidate-test",
                    config=_trainer.TrainerConfig(min_samples=8),
                )


if __name__ == "__main__":
    unittest.main()
