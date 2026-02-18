# NeuroHID

[![CI](https://github.com/jmduea/NeuroHID/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jmduea/NeuroHID/actions/workflows/ci.yml?query=branch%3Amain)
[![codecov](https://codecov.io/gh/jmduea/NeuroHID/branch/main/graph/badge.svg)](https://codecov.io/gh/jmduea/NeuroHID)

**Transform consumer EEG devices into standard PC peripherals using deep reinforcement learning.**

NeuroHID is a local-first brain-computer interface system that decodes intent from EEG signals and
translates it into standard mouse and keyboard actions. Applications do not need integration with
NeuroHID; they receive normal HID input events.

## Vision

Put on a lightweight EEG headset, think about a movement, and get usable computer control through a
standard input stack. NeuroHID is designed to continuously adapt from ongoing usage signals,
including implicit error-related feedback, instead of relying on one-time calibration alone.

## Architecture

NeuroHID uses a hybrid Rust/Python architecture:

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           RUST CORE SERVICE                             │
│                     (neurohid-core + related crates)                    │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────────┐     │
│  │ Device       │  │ Signal       │  │ Platform (HID Emulation)   │     │
│  │ Backends     │──│ Processing   │  │ Linux / Windows / macOS    │     │
│  │ (LSL/Serial/ │  │ Pipeline     │  └─────────────┬──────────────┘     │
│  │ BrainFlow/   │  │              │                │                    │
│  │ Mock/Auto)   │  │              │                │                    │
│  └──────┬───────┘  └──────┬───────┘                │                    │
│         │                 │                        │                    │
│    EEG Samples        Features                  Actions                 │
│         │                 │                        ▲                    │
│         ▼                 ▼                        │                    │
│  ┌────────────────────────────────────┐    ┌───────┴────────┐           │
│  │         Ring Buffer / State        │    │ Action Executor│           │
│  └──────────────────┬─────────────────┘    └───────▲────────┘           │
│                     │                              │                    │
│                     │ IPC (Local Socket)           │                    │
├─────────────────────┼──────────────────────────────┼────────────────────┤
│                     │     PYTHON ML LAYER          │                    │
├─────────────────────┼──────────────────────────────┼────────────────────┤
│                     ▼                              │                    │
│  ┌────────────────────────────────────┐            │                    │
│  │           IPC Client               │            │                    │
│  └──────────────────┬─────────────────┘            │                    │
│                     │                              │                    │
│         ┌───────────┴───────────┐                  │                    │
│         ▼                       ▼                  │                    │
│  ┌──────────────┐       ┌──────────────┐           │                    │
│  │ ErrP Detector│       │   Decoder    │───────────┘                    │
│  │ (Classifier) │       │ (PPO Policy) │                                │
│  └──────────────┘       └──────────────┘                                │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Why Rust + Python

- Rust handles latency-critical runtime paths (device ingestion, signal transforms, HID output).
- Python provides rapid iteration for model experimentation and training workflows.
- The IPC boundary isolates failures and keeps runtime behavior resilient when ML components are
  unavailable.

## Key Innovations

### Error-Related Potentials as Reward

NeuroHID uses error-related potentials (ErrPs) as implicit reinforcement signals. Instead of asking
for explicit "correct/incorrect" feedback, the system can learn from neural responses to undesired
output behavior.

### Continuous Online Learning

Signal characteristics and interaction patterns drift over time. NeuroHID is designed for
continuous adaptation rather than static, train-once decoding.

### Zero Application Integration

NeuroHID emits standard HID events, so existing applications can be used without app-specific BCI
integration.

## Status and Scope

NeuroHID is an actively developed pre-production monorepo with a Rust runtime/control surface and a
Python ML package. The project is currently optimized for local operation and research/developer
iteration workflows.

## Documentation Map

- Project index: [`docs/index.md`](./docs/index.md)
- Project overview: [`docs/project-overview.md`](./docs/project-overview.md)
- Source tree map: [`docs/source-tree-analysis.md`](./docs/source-tree-analysis.md)
- Rust core architecture: [`docs/architecture-rust-core.md`](./docs/architecture-rust-core.md)
- Python ML architecture: [`docs/architecture-python-ml.md`](./docs/architecture-python-ml.md)
- Integration architecture: [`docs/integration-architecture.md`](./docs/integration-architecture.md)
- Development workflows: [`docs/development-guide.md`](./docs/development-guide.md)
- Deployment/operations workflows: [`docs/deployment-guide.md`](./docs/deployment-guide.md)
- Python package usage: [`python/README.md`](./python/README.md)
- Contribution process: [`CONTRIBUTING.md`](./CONTRIBUTING.md)
- Release history: [`CHANGELOG.md`](./CHANGELOG.md)

## Roadmap Source of Truth

Roadmap status is tracked in GitHub:

- Issues: <https://github.com/jmduea/NeuroHID/issues>
- Milestones: <https://github.com/jmduea/NeuroHID/milestones>

## License

This project is dual-licensed under MIT or Apache 2.0, your choice.
