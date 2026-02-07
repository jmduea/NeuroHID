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

from neurohid_ml.decoder import Decoder, DecoderConfig
from neurohid_ml.errp import ErrPDetector, ErrPConfig
from neurohid_ml.bridge import IpcClient

__all__ = [
    "Decoder",
    "DecoderConfig", 
    "ErrPDetector",
    "ErrPConfig",
    "IpcClient",
]
