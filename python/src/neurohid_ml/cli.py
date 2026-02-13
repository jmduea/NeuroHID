"""Command-line entrypoint for neurohid-ml."""

from __future__ import annotations

import argparse
import asyncio
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Sequence

from neurohid_ml.bridge import main_async as bridge_main_async
from neurohid_ml.trainer import TrainerConfig, train_candidate_model


def _add_training_hyperparameter_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--model-version", type=str, default="candidate-0")
    parser.add_argument("--epochs", type=int, default=10)
    parser.add_argument("--learning-rate", type=float, default=1e-3)
    parser.add_argument("--holdout-ratio", type=float, default=0.2)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--decode-latency-p95-us", type=int, default=40_000)
    parser.add_argument("--min-samples", type=int, default=64)


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
    _add_training_hyperparameter_args(trainer)
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

    profile_trainer = subparsers.add_parser(
        "train-profile-candidate",
        help="export profile sessions, train candidate artifacts, and stage into profile storage",
    )
    profile_trainer.add_argument("--profile-id", type=str, required=True)
    profile_trainer.add_argument(
        "--service-bin",
        type=str,
        default="neurohid-service",
        help="service binary used for session export and candidate staging",
    )
    profile_trainer.add_argument(
        "--work-dir",
        type=Path,
        help="working directory for plaintext exports and trainer outputs",
    )
    profile_trainer.add_argument(
        "--output-dir",
        type=Path,
        help="candidate artifact output directory (defaults to <work-dir>/candidate)",
    )
    profile_trainer.add_argument(
        "--keep-work-dir",
        action="store_true",
        help="keep temporary work directory after completion",
    )
    _add_training_hyperparameter_args(profile_trainer)

    return parser.parse_args(argv_list)


def _trainer_config_from_args(args: argparse.Namespace) -> TrainerConfig:
    return TrainerConfig(
        epochs=args.epochs,
        learning_rate=args.learning_rate,
        holdout_ratio=args.holdout_ratio,
        seed=args.seed,
        decode_latency_p95_us=args.decode_latency_p95_us,
        min_samples=args.min_samples,
    )


def _print_training_outputs(output_dir: Path, args: argparse.Namespace) -> None:
    session_logs = [Path(p) for p in getattr(args, "session_log", [])]
    if args.session_dir:
        session_logs.extend(sorted(args.session_dir.glob("session_*.json")))
    if not session_logs:
        raise SystemExit("No session logs supplied. Use --session-log and/or --session-dir.")

    outputs = train_candidate_model(
        session_logs=session_logs,
        output_dir=output_dir,
        model_version=args.model_version,
        config=_trainer_config_from_args(args),
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


def _run_command(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(cmd, check=False, text=True, capture_output=True)
    if completed.stdout:
        print(completed.stdout, end="")
    if completed.stderr:
        print(completed.stderr, end="", file=sys.stderr)
    return completed


def _stage_candidate(service_bin: str, profile_id: str, output_dir: Path) -> None:
    cmd = [
        service_bin,
        "--profile",
        profile_id,
        "--import-candidate-dir",
        str(output_dir),
    ]
    completed = _run_command(cmd)
    if completed.returncode != 0:
        raise SystemExit(
            "Candidate staging failed. "
            f"Command exited with {completed.returncode}: {' '.join(cmd)}"
        )
    print(f"Staged candidate artifacts for profile: {profile_id}")


def _run_train_candidate(args: argparse.Namespace) -> None:
    _print_training_outputs(args.output_dir, args)
    if args.stage_profile_id:
        _stage_candidate(args.service_bin, args.stage_profile_id, args.output_dir)


def _run_train_profile_candidate(args: argparse.Namespace) -> None:
    transient_root = False
    if args.work_dir:
        work_root = args.work_dir
    else:
        work_root = Path(tempfile.mkdtemp(prefix="neurohid_ml_train_"))
        transient_root = True
    work_root.mkdir(parents=True, exist_ok=True)

    session_dir = work_root / "sessions"
    output_dir = args.output_dir or (work_root / "candidate")
    session_dir.mkdir(parents=True, exist_ok=True)
    output_dir.mkdir(parents=True, exist_ok=True)

    export_cmd = [
        args.service_bin,
        "--profile",
        args.profile_id,
        "--export-session-logs-dir",
        str(session_dir),
    ]

    try:
        export_completed = _run_command(export_cmd)
        if export_completed.returncode != 0:
            raise SystemExit(
                "Session export failed. "
                f"Command exited with {export_completed.returncode}: {' '.join(export_cmd)}"
            )

        exported_logs = sorted(session_dir.glob("session_*.json"))
        if not exported_logs:
            raise SystemExit(
                f"No exported sessions found for profile '{args.profile_id}' in {session_dir}"
            )

        trainer_args = argparse.Namespace(**vars(args))
        trainer_args.session_log = []
        trainer_args.session_dir = session_dir
        _print_training_outputs(output_dir, trainer_args)

        _stage_candidate(args.service_bin, args.profile_id, output_dir)
    finally:
        if transient_root and not args.keep_work_dir:
            shutil.rmtree(work_root, ignore_errors=True)

    if transient_root and args.keep_work_dir:
        print(f"Kept work directory: {work_root}")
    elif args.work_dir:
        print(f"Work directory: {work_root}")


def main(argv: Sequence[str] | None = None) -> None:
    args = _parse_args(argv)

    if args.command == "bridge":
        asyncio.run(bridge_main_async(args.host, args.port))
        return

    if args.command == "train-candidate":
        _run_train_candidate(args)
        return

    if args.command == "train-profile-candidate":
        _run_train_profile_candidate(args)
        return

    raise SystemExit(f"Unknown command: {args.command}")


if __name__ == "__main__":
    main()
