"""
NeuroHID ML - Python Machine Learning Components

This package contains the machine learning components for NeuroHID:
- decoder: The RL policy network (PPO) that translates brain signals to actions
- errp: Error-Related Potential detection for implicit feedback
- bridge: In-process trainer bridge communicating via RuntimeHandle

All runtime communication uses the ``neurohid`` native extension module's
in-process ``RuntimeHandle`` rather than socket-based IPC.
"""

__version__ = "0.1.0"

__all__ = [
    "Decoder",
    "DecoderConfig",
    "ErrPDetector",
    "ErrPConfig",
    "IpcClient",
    "NeuroHidControlClient",
    "NeuroHidNotebook",
    "NeuroHidTelemetryClient",
    "NotebookError",
    "TrainerConfig",
    "train_candidate_model",
]


def __getattr__(name: str):
    if name == "IpcClient":
        from neurohid_ml.bridge import IpcClient

        globals().update({"IpcClient": IpcClient})
        return IpcClient

    if name in {"Decoder", "DecoderConfig"}:
        from neurohid_ml.decoder import Decoder, DecoderConfig

        globals().update({"Decoder": Decoder, "DecoderConfig": DecoderConfig})
        return globals()[name]

    if name in {"ErrPDetector", "ErrPConfig"}:
        from neurohid_ml.errp import ErrPConfig, ErrPDetector

        globals().update({"ErrPDetector": ErrPDetector, "ErrPConfig": ErrPConfig})
        return globals()[name]

    if name in {"TrainerConfig", "train_candidate_model"}:
        from neurohid_ml.trainer import TrainerConfig, train_candidate_model

        globals().update(
            {
                "TrainerConfig": TrainerConfig,
                "train_candidate_model": train_candidate_model,
            }
        )
        return globals()[name]

    if name in {"NeuroHidNotebook", "NotebookError"}:
        from neurohid_ml.control import NotebookError
        from neurohid_ml.notebook import NeuroHidNotebook

        globals().update(
            {
                "NeuroHidNotebook": NeuroHidNotebook,
                "NotebookError": NotebookError,
            }
        )
        return globals()[name]

    if name == "NeuroHidControlClient":
        from neurohid_ml.control import NeuroHidControlClient

        globals().update({"NeuroHidControlClient": NeuroHidControlClient})
        return NeuroHidControlClient

    if name == "NeuroHidTelemetryClient":
        from neurohid_ml.telemetry import NeuroHidTelemetryClient

        globals().update({"NeuroHidTelemetryClient": NeuroHidTelemetryClient})
        return NeuroHidTelemetryClient

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
