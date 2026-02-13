from __future__ import annotations

import importlib
import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_decoder = importlib.import_module("neurohid_ml.decoder")
_errp = importlib.import_module("neurohid_ml.errp")


class DecoderAndErrpTests(unittest.TestCase):
    def test_decoder_get_action_and_train_step_behavior(self) -> None:
        config = _decoder.DecoderConfig(input_dim=4, hidden_dims=[8], batch_size=2)
        decoder = _decoder.Decoder(config)

        action = decoder.get_action(np.asarray([0.1, -0.2, 0.3, 0.0], dtype=np.float32))
        self.assertIn("mouse_dx", action)
        self.assertIn("mouse_dy", action)
        self.assertIn("discrete", action)
        self.assertIn("confidence", action)

        decoder.add_experience(np.zeros(4, dtype=np.float32), action, reward=0.0, done=False)
        self.assertIsNone(decoder.train_step())

    def test_errp_fit_and_detect(self) -> None:
        detector = _errp.ErrPDetector(_errp.ErrPConfig(num_channels=5))
        rng = np.random.default_rng(7)

        for i in range(12):
            window = rng.normal(0.0, 1.0, size=(64, 5)).astype(np.float32)
            detector.add_calibration_example(window, was_error=(i % 2 == 0))

        fit_metrics = detector.fit()
        self.assertGreaterEqual(fit_metrics["num_examples"], 12)

        detection = detector.detect(rng.normal(0.0, 1.0, size=(64, 5)).astype(np.float32))
        self.assertGreaterEqual(detection.error_probability, 0.0)
        self.assertLessEqual(detection.error_probability, 1.0)
        self.assertGreaterEqual(detection.confidence, 0.0)
        self.assertLessEqual(detection.confidence, 1.0)


if __name__ == "__main__":
    unittest.main()
