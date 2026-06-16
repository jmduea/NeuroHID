"""Command-line entrypoint for neurohid-ml."""

from __future__ import annotations

import argparse
import asyncio
import json
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Sequence


def _add_training_hyperparameter_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--model-version", type=str, default="candidate-0")
    parser.add_argument("--epochs", type=int, default=10)
    parser.add_argument("--learning-rate", type=float, default=1e-3)
    parser.add_argument("--holdout-ratio", type=float, default=0.2)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--decode-latency-p95-us", type=int, default=40_000)
    parser.add_argument("--min-samples", type=int, default=64)


def _parse_bool_literal(value: str) -> bool:
    lowered = value.strip().lower()
    if lowered in {"1", "true", "yes", "on"}:
        return True
    if lowered in {"0", "false", "no", "off"}:
        return False
    raise argparse.ArgumentTypeError("expected one of: true/false, yes/no, on/off, 1/0")


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    argv_list = list(argv) if argv is not None else sys.argv[1:]
    if not argv_list:
        argv_list = ["bridge", *argv_list]
    elif argv_list[0].startswith("-") and argv_list[0] not in {"-h", "--help"}:
        argv_list = ["bridge", *argv_list]

    parser = argparse.ArgumentParser(description="NeuroHID ML tools")
    subparsers = parser.add_subparsers(dest="command")

    bridge = subparsers.add_parser("bridge", help="run the in-process trainer bridge")
    bridge.add_argument(
        "--config-json",
        type=str,
        default=None,
        help="optional JSON object for SystemConfig to start the runtime",
    )

    control = subparsers.add_parser(
        "control",
        help="send one control command (requires an in-process runtime)",
    )
    control.add_argument(
        "action",
        choices=[
            "snapshot",
            "trainer_snapshot",
            "set_output_enabled",
            "set_learning_enabled",
            "set_fallback_policy",
            "ml_bridge_reconnect",
            "reload_model",
            "promote_candidate_model",
            "rescan_streams",
            "connect_stream",
            "disconnect_stream",
            "ensure_connected_stream",
        ],
    )
    control.add_argument("--enabled", type=_parse_bool_literal)
    control.add_argument(
        "--policy-json",
        type=str,
        help="JSON object for set_fallback_policy command",
    )
    control.add_argument("--stream-id", type=str)
    control.add_argument(
        "--config-json",
        type=str,
        default=None,
        help="optional JSON object for SystemConfig to start the runtime",
    )

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

    worker = subparsers.add_parser(
        "trainer-worker",
        help="poll profile sessions and continuously train/stage candidate models",
    )
    worker.add_argument("--profile-id", type=str, required=True)
    worker.add_argument(
        "--service-bin",
        type=str,
        default="neurohid-service",
        help="service binary used for session export and candidate staging",
    )
    worker.add_argument(
        "--work-dir",
        type=Path,
        help="working directory for plaintext exports and trainer outputs",
    )
    worker.add_argument(
        "--output-dir",
        type=Path,
        help="candidate artifact output directory (defaults to <work-dir>/candidate)",
    )
    worker.add_argument(
        "--keep-work-dir",
        action="store_true",
        help="keep temporary work directory after each loop iteration",
    )
    worker.add_argument(
        "--poll-interval-secs",
        type=int,
        default=120,
        help="sleep interval between polling iterations",
    )
    worker.add_argument(
        "--min-session-count",
        type=int,
        default=1,
        help="minimum exported sessions required before training",
    )
    worker.add_argument(
        "--once",
        action="store_true",
        help="run one polling iteration and exit",
    )
    _add_training_hyperparameter_args(worker)

    lab_kernel = subparsers.add_parser(
        "lab-kernel",
        help="run stdio notebook-kernel adapter for in-app Python Lab",
    )
    lab_kernel.add_argument(
        "--stdio",
        action="store_true",
        help="use JSON-lines stdio protocol (default behavior)",
    )

    return parser.parse_args(argv_list)


def _trainer_config_from_args(args: argparse.Namespace):
    from neurohid_ml.trainer import TrainerConfig

    return TrainerConfig(
        epochs=args.epochs,
        learning_rate=args.learning_rate,
        holdout_ratio=args.holdout_ratio,
        seed=args.seed,
        decode_latency_p95_us=args.decode_latency_p95_us,
        min_samples=args.min_samples,
    )


def _print_training_outputs(output_dir: Path, args: argparse.Namespace) -> None:
    from neurohid_ml.trainer import train_candidate_model

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


def _run_trainer_worker(args: argparse.Namespace) -> None:
    if args.poll_interval_secs < 1:
        raise SystemExit("--poll-interval-secs must be >= 1")
    if args.min_session_count < 1:
        raise SystemExit("--min-session-count must be >= 1")

    seen_signature: tuple[str, ...] | None = None

    while True:
        transient_root = False
        if args.work_dir:
            work_root = args.work_dir
        else:
            work_root = Path(tempfile.mkdtemp(prefix="neurohid_ml_worker_"))
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
        print(f"[trainer-worker] Exporting sessions for profile '{args.profile_id}'")
        export_completed = _run_command(export_cmd)
        if export_completed.returncode != 0:
            print(
                "[trainer-worker] Session export failed " f"(exit {export_completed.returncode})."
            )
        else:
            session_logs = sorted(session_dir.glob("session_*.json"))
            signature = tuple(path.name for path in session_logs)

            if len(session_logs) < args.min_session_count:
                print(
                    "[trainer-worker] Skipping training: "
                    f"{len(session_logs)} session(s) < minimum {args.min_session_count}"
                )
            elif signature == seen_signature:
                print("[trainer-worker] No new sessions since last successful training")
            else:
                trainer_args = argparse.Namespace(**vars(args))
                trainer_args.session_log = []
                trainer_args.session_dir = session_dir
                trainer_args.output_dir = output_dir

                try:
                    print(
                        "[trainer-worker] Training and staging candidate from "
                        f"{len(session_logs)} session(s)"
                    )
                    _print_training_outputs(output_dir, trainer_args)
                    _stage_candidate(args.service_bin, args.profile_id, output_dir)
                    seen_signature = signature
                    print("[trainer-worker] Candidate staged successfully")
                except SystemExit as error:
                    print(f"[trainer-worker] Training/staging failed: {error}")

        if transient_root and not args.keep_work_dir:
            shutil.rmtree(work_root, ignore_errors=True)
        elif transient_root and args.keep_work_dir:
            print(f"[trainer-worker] Kept work directory: {work_root}")

        if args.once:
            break

        time.sleep(args.poll_interval_secs)


def _start_runtime_from_args(args: argparse.Namespace):
    """Start an in-process runtime from optional --config-json CLI argument."""
    from neurohid import RuntimeBuilder, SystemConfig

    config_json = getattr(args, "config_json", None)
    if config_json:
        config = SystemConfig.from_dict(json.loads(config_json))
    else:
        config = SystemConfig.from_dict({})

    async def _start():
        return await RuntimeBuilder(config).start()

    return asyncio.run(_start())


def _run_control(args: argparse.Namespace) -> None:
    from neurohid_ml.control import NeuroHidControlClient

    runtime = _start_runtime_from_args(args)
    client = NeuroHidControlClient(runtime)

    if args.action == "snapshot":
        print(json.dumps(client.snapshot(), indent=2, sort_keys=True))
        return
    if args.action == "trainer_snapshot":
        print(json.dumps(client.trainer_snapshot(), indent=2, sort_keys=True))
        return
    if args.action == "set_output_enabled":
        if args.enabled is None:
            raise SystemExit("--enabled is required for set_output_enabled")
        print(json.dumps(client.set_output_enabled(args.enabled), indent=2, sort_keys=True))
        return
    if args.action == "set_learning_enabled":
        if args.enabled is None:
            raise SystemExit("--enabled is required for set_learning_enabled")
        print(json.dumps(client.set_learning_enabled(args.enabled), indent=2, sort_keys=True))
        return
    if args.action == "set_fallback_policy":
        if not args.policy_json:
            raise SystemExit("--policy-json is required for set_fallback_policy")
        try:
            policy = json.loads(args.policy_json)
        except json.JSONDecodeError as error:
            raise SystemExit(f"invalid --policy-json: {error}") from error
        if not isinstance(policy, dict):
            raise SystemExit("--policy-json must decode to a JSON object")
        print(json.dumps(client.set_fallback_policy(policy), indent=2, sort_keys=True))
        return
    if args.action == "ml_bridge_reconnect":
        print(json.dumps(client.reconnect_bridge(), indent=2, sort_keys=True))
        return
    if args.action == "reload_model":
        print(json.dumps(client.reload_model(), indent=2, sort_keys=True))
        return
    if args.action == "promote_candidate_model":
        print(json.dumps(client.promote_candidate_model(), indent=2, sort_keys=True))
        return
    if args.action == "rescan_streams":
        print(json.dumps(client.rescan_streams(), indent=2, sort_keys=True))
        return
    if args.action == "connect_stream":
        if not args.stream_id:
            raise SystemExit("--stream-id is required for connect_stream")
        print(json.dumps(client.connect_stream(args.stream_id), indent=2, sort_keys=True))
        return
    if args.action == "disconnect_stream":
        if not args.stream_id:
            raise SystemExit("--stream-id is required for disconnect_stream")
        print(json.dumps(client.disconnect_stream(args.stream_id), indent=2, sort_keys=True))
        return
    if args.action == "ensure_connected_stream":
        print(json.dumps({"stream_id": client.ensure_connected_stream()}, indent=2))
        return

    raise SystemExit(f"Unknown control action: {args.action}")


def main(argv: Sequence[str] | None = None) -> None:
    args = _parse_args(argv)

    if args.command == "bridge":
        from neurohid_ml.bridge import main_async as bridge_main_async

        runtime = _start_runtime_from_args(args)
        asyncio.run(bridge_main_async(runtime))
        return

    if args.command == "train-candidate":
        _run_train_candidate(args)
        return

    if args.command == "train-profile-candidate":
        _run_train_profile_candidate(args)
        return

    if args.command == "trainer-worker":
        _run_trainer_worker(args)
        return

    if args.command == "lab-kernel":
        from neurohid_ml.lab_kernel import run_stdio

        if not args.stdio:
            print("lab-kernel defaults to stdio protocol; running stdio adapter.")
        run_stdio()
        return

    if args.command == "control":
        _run_control(args)
        return

    raise SystemExit(f"Unknown command: {args.command}")


if __name__ == "__main__":
    main()
