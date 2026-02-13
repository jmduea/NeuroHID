"""Command-line entrypoint for neurohid-ml."""

from __future__ import annotations

import argparse
import asyncio
from pathlib import Path
import subprocess
import sys
from typing import Sequence

from neurohid_ml.bridge import main_async as bridge_main_async
from neurohid_ml.trainer import TrainerConfig, train_candidate_model


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    argv_list = list(argv) if argv is not None else sys.argv[1:]
    if not argv_list or argv_list[0].startswith("-"):
        argv_list = ["bridge", *argv_list]

    parser = argparse.ArgumentParser(description="NeuroHID ML tools")
    subparsers = parser.add_subparsers(dest="command")

    bridge = subparsers.add_parser("bridge", help="run the realtime IPC bridge")
    bridge.add_argument("--host", default="127.0.0.1")
    bridge.add_argument("--port", type=int, default=47384)

    trainer = subparsers.add_parser(
        "train-candidate",
        help="train a candidate decoder model from session logs",
    )
    trainer.add_argument(
        "--session-log",
        action="append",
        default=[],
        help="path to a plaintext TrainingSessionLog json file (repeatable)",
    )
    trainer.add_argument(
        "--session-dir",
        type=Path,
        help="directory containing plaintext session_*.json files",
    )
    trainer.add_argument("--output-dir", type=Path, required=True)
    trainer.add_argument("--model-version", type=str, default="candidate-0")
    trainer.add_argument("--epochs", type=int, default=10)
    trainer.add_argument("--learning-rate", type=float, default=1e-3)
    trainer.add_argument("--holdout-ratio", type=float, default=0.2)
    trainer.add_argument("--seed", type=int, default=7)
    trainer.add_argument("--decode-latency-p95-us", type=int, default=40_000)
    trainer.add_argument("--min-samples", type=int, default=64)
    trainer.add_argument(
        "--stage-profile-id",
        type=str,
        help="if set, stage produced candidate artifacts into encrypted profile storage",
    )
    trainer.add_argument(
        "--service-bin",
        type=str,
        default="neurohid-service",
        help="service binary used for staging candidate artifacts",
    )

    return parser.parse_args(argv_list)


def main(argv: Sequence[str] | None = None) -> None:
    args = _parse_args(argv)

    if args.command == "bridge":
        asyncio.run(bridge_main_async(args.host, args.port))
        return

    if args.command == "train-candidate":
        session_logs = [Path(p) for p in args.session_log]
        if args.session_dir:
            session_logs.extend(sorted(args.session_dir.glob("session_*.json")))
        if not session_logs:
            raise SystemExit(
                "No session logs supplied. Use --session-log and/or --session-dir."
            )

        config = TrainerConfig(
            epochs=args.epochs,
            learning_rate=args.learning_rate,
            holdout_ratio=args.holdout_ratio,
            seed=args.seed,
            decode_latency_p95_us=args.decode_latency_p95_us,
            min_samples=args.min_samples,
        )
        outputs = train_candidate_model(
            session_logs=session_logs,
            output_dir=args.output_dir,
            model_version=args.model_version,
            config=config,
        )
        print(f"Wrote ONNX: {outputs.onnx_path}")
        print(f"Wrote manifest: {outputs.manifest_path}")
        print(f"Wrote metrics: {outputs.metrics_path}")
        print(
            "Holdout: "
            f"n={outputs.holdout_sample_count} "
            f"acc={outputs.holdout_accuracy:.4f} "
            f"loss={outputs.holdout_loss:.4f}"
        )

        if args.stage_profile_id:
            cmd = [
                args.service_bin,
                "--profile",
                args.stage_profile_id,
                "--import-candidate-dir",
                str(args.output_dir),
            ]
            completed = subprocess.run(cmd, check=False)
            if completed.returncode != 0:
                raise SystemExit(
                    "Candidate staging failed. "
                    f"Command exited with {completed.returncode}: {' '.join(cmd)}"
                )
            print(f"Staged candidate artifacts for profile: {args.stage_profile_id}")
        return

    raise SystemExit(f"Unknown command: {args.command}")


if __name__ == "__main__":
    main()
