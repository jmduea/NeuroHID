"""
NeuroHID ML - Python Machine Learning Components

This package contains the machine learning components for NeuroHID:
- decoder: The RL policy network (PPO) that translates brain signals to actions
- errp: Error-Related Potential detection for implicit feedback
- bridge: IPC client for communicating with the Rust core service

The package is designed to run as a separate process from the Rust core,
communicating via a local socket. This architecture provides:
1. Process isolation (Python crashes don't stop the Rust service)
2. Full access to the PyTorch ecosystem
3. Hot-reloading of ML code without restarting the service
"""

__version__ = "0.1.0"

__all__ = [
    "Decoder",
    "DecoderConfig",
    "ErrPDetector",
    "ErrPConfig",
    "IpcClient",
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

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
