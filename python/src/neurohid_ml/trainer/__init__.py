"""Candidate model trainer for NeuroHID continuous learning.

This module consumes training session logs and produces candidate artifacts:
- decoder_candidate.onnx
- decoder_candidate_manifest.json
- decoder_candidate_metrics.json

The generated artifacts are intended for guarded promotion by the Rust runtime.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import time
from typing import Iterable, List, Sequence

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F


@dataclass
class TrainerConfig:
    """Configuration for candidate training."""

    epochs: int = 10
    learning_rate: float = 1e-3
    holdout_ratio: float = 0.2
    seed: int = 7
    decode_latency_p95_us: int = 40_000
    min_samples: int = 64


@dataclass
class TrainingOutputs:
    """Locations and summary of emitted candidate artifacts."""

    onnx_path: Path
    manifest_path: Path
    metrics_path: Path
    holdout_accuracy: float
    holdout_loss: float
    holdout_sample_count: int


class CandidateDecoderNet(nn.Module):
    """Simple feedforward model for candidate decoder training."""

    def __init__(self, input_dim: int):
        super().__init__()
        self.net = nn.Sequential(
            nn.Linear(input_dim, 128),
            nn.ReLU(),
            nn.Linear(128, 64),
            nn.ReLU(),
            nn.Linear(64, 5),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.net(x)


def train_candidate_model(
    session_logs: Sequence[Path],
    output_dir: Path,
    model_version: str,
    config: TrainerConfig | None = None,
) -> TrainingOutputs:
    """Train a candidate model from session logs and write artifacts."""

    cfg = config or TrainerConfig()
    output_dir.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(cfg.seed)
    torch.manual_seed(cfg.seed)

    features, targets = _load_dataset(session_logs)
    if features.shape[0] < cfg.min_samples:
        raise ValueError(
            f"insufficient samples for candidate training: have {features.shape[0]}, "
            f"need at least {cfg.min_samples}"
        )

    indices = rng.permutation(features.shape[0])
    holdout_count = int(max(1, round(features.shape[0] * cfg.holdout_ratio)))
    holdout_indices = indices[:holdout_count]
    train_indices = indices[holdout_count:]
    if train_indices.size == 0:
        train_indices = holdout_indices

    x_train = features[train_indices]
    y_train = targets[train_indices]
    x_holdout = features[holdout_indices]
    y_holdout = targets[holdout_indices]

    mean = x_train.mean(axis=0).astype(np.float32)
    std = x_train.std(axis=0).astype(np.float32)
    std = np.where(std < 1e-6, 1.0, std).astype(np.float32)

    x_train_norm = _normalize(x_train, mean, std)
    x_holdout_norm = _normalize(x_holdout, mean, std)

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model = CandidateDecoderNet(input_dim=x_train.shape[1]).to(device)
    optimizer = torch.optim.Adam(model.parameters(), lr=cfg.learning_rate)

    train_x_t = torch.tensor(x_train_norm, dtype=torch.float32, device=device)
    train_y_t = torch.tensor(y_train, dtype=torch.float32, device=device)
    holdout_x_t = torch.tensor(x_holdout_norm, dtype=torch.float32, device=device)
    holdout_y_t = torch.tensor(y_holdout, dtype=torch.float32, device=device)

    for _ in range(max(1, cfg.epochs)):
        model.train()
        optimizer.zero_grad(set_to_none=True)
        pred = model(train_x_t)
        loss = _composite_loss(pred, train_y_t)
        loss.backward()
        optimizer.step()

    model.eval()
    with torch.no_grad():
        holdout_pred = model(holdout_x_t)
        holdout_loss = float(_composite_loss(holdout_pred, holdout_y_t).item())
        holdout_accuracy = float(_holdout_accuracy(holdout_pred, holdout_y_t))

    onnx_path = output_dir / "decoder_candidate.onnx"
    manifest_path = output_dir / "decoder_candidate_manifest.json"
    metrics_path = output_dir / "decoder_candidate_metrics.json"

    example = torch.randn(1, x_train.shape[1], dtype=torch.float32, device=device)
    torch.onnx.export(
        model,
        example,
        str(onnx_path),
        input_names=["input"],
        output_names=["output"],
        opset_version=17,
    )

    trained_at = _now_micros()
    manifest = {
        "model_version": model_version,
        "input_dim": int(x_train.shape[1]),
        "feature_schema_version": 1,
        "action_schema_version": 1,
        "normalization_stats": {
            "mean": mean.tolist(),
            "std": std.tolist(),
        },
        "trained_at": trained_at,
    }
    metrics = {
        "holdout_sample_count": int(x_holdout.shape[0]),
        "holdout_accuracy": holdout_accuracy,
        "holdout_loss": holdout_loss,
        "decode_latency_p95_us": int(cfg.decode_latency_p95_us),
        "generated_at": trained_at,
    }

    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    metrics_path.write_text(json.dumps(metrics, indent=2), encoding="utf-8")

    return TrainingOutputs(
        onnx_path=onnx_path,
        manifest_path=manifest_path,
        metrics_path=metrics_path,
        holdout_accuracy=holdout_accuracy,
        holdout_loss=holdout_loss,
        holdout_sample_count=int(x_holdout.shape[0]),
    )


def _load_dataset(session_logs: Sequence[Path]) -> tuple[np.ndarray, np.ndarray]:
    feature_rows: List[np.ndarray] = []
    target_rows: List[np.ndarray] = []
    expected_dim: int | None = None

    for path in session_logs:
        payload = json.loads(path.read_text(encoding="utf-8"))
        episodes = payload.get("episodes", [])
        for episode in episodes:
            features = np.asarray(episode.get("feature_values", []), dtype=np.float32)
            if features.size == 0:
                continue

            if expected_dim is None:
                expected_dim = int(features.size)
            if features.size != expected_dim:
                continue

            target_rows.append(_extract_target_vector(episode))
            feature_rows.append(features)

    if not feature_rows:
        raise ValueError("no usable episodes found in session logs")

    feature_matrix = np.stack(feature_rows, axis=0).astype(np.float32)
    target_matrix = np.stack(target_rows, axis=0).astype(np.float32)
    return feature_matrix, target_matrix


def _extract_target_vector(episode: dict) -> np.ndarray:
    action = episode.get("action", {}) or {}
    mouse = action.get("mouse", {}) or {}
    movement = mouse.get("movement", {}) or {}
    buttons = mouse.get("buttons", []) or []

    dx = float(movement.get("dx", 0.0) or 0.0)
    dy = float(movement.get("dy", 0.0) or 0.0)
    left_click = 1.0 if _pressed(buttons, "Left") else 0.0
    right_click = 1.0 if _pressed(buttons, "Right") else 0.0
    confidence = float(episode.get("decoder_confidence", action.get("confidence", 0.0)) or 0.0)

    return np.asarray([dx, dy, left_click, right_click, confidence], dtype=np.float32)


def _pressed(button_events: Iterable[dict], button_name: str) -> bool:
    for ev in button_events:
        if ev.get("pressed") is True and str(ev.get("button")) == button_name:
            return True
    return False


def _normalize(values: np.ndarray, mean: np.ndarray, std: np.ndarray) -> np.ndarray:
    return np.clip((values - mean) / std, -10.0, 10.0).astype(np.float32)


def _composite_loss(pred: torch.Tensor, target: torch.Tensor) -> torch.Tensor:
    move_loss = F.mse_loss(pred[:, :2], target[:, :2])
    click_loss = F.binary_cross_entropy_with_logits(pred[:, 2:4], target[:, 2:4])
    conf_loss = F.binary_cross_entropy_with_logits(pred[:, 4], target[:, 4])
    return move_loss + click_loss + 0.5 * conf_loss


def _holdout_accuracy(pred: torch.Tensor, target: torch.Tensor) -> float:
    click_pred = (torch.sigmoid(pred[:, 2:4]) >= 0.5).to(torch.float32)
    click_true = target[:, 2:4]
    click_acc = float((click_pred == click_true).to(torch.float32).mean().item())

    move_mae = float(torch.abs(pred[:, :2] - target[:, :2]).mean().item())
    move_score = float(np.exp(-move_mae))

    return max(0.0, min(1.0, 0.7 * click_acc + 0.3 * move_score))


def _now_micros() -> int:
    return int(time.time() * 1_000_000)
