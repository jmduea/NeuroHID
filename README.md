# NeuroHID

[![CI](https://github.com/jmduea/neurohid/actions/workflows/ci.yml/badge.svg)](https://github.com/jmduea/neurohid/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/jmduea/neurohid/branch/main/graph/badge.svg)](https://codecov.io/gh/jmduea/neurohid)

**Transform consumer EEG devices into standard PC peripherals using deep reinforcement learning.**

NeuroHID is a system that learns to decode your intentions from brain signals and translates them into mouse movements, clicks, and keyboard inputs. It runs as a background service, requiring no application integration—your computer just gains a new input device that happens to be controlled by your thoughts.

## Vision

Imagine putting on a lightweight EEG headset, thinking "move left," and watching your cursor smoothly glide across the screen. No training wheels, no special applications, no steep learning curve. NeuroHID continuously learns from the implicit feedback your brain generates when actions don't match your intentions, getting better the more you use it.

## Architecture

NeuroHID uses a hybrid Rust/Python architecture:

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           RUST CORE SERVICE                             │
│                     (neurohid-core + related crates)                    │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────────┐     │
│  │ Device       │  │ Signal       │  │ Platform (HID Emulation)   │     │
│  │ Abstraction  │──│ Processing   │  │ Linux / Windows / macOS    │     │
│  │ (Emotiv API) │  │ Pipeline     │  └─────────────┬──────────────┘     │
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

### Why This Split?

**Rust handles the latency-critical path:**

- Every EEG sample arrives every ~8ms and must be processed immediately
- HID events must be emitted with minimal, consistent latency
- The background service must never stutter or pause (no GC!)

**Python handles the ML:**

- PyTorch ecosystem for neural networks
- Rapid experimentation with model architectures
- The ML community lives in Python—contributors need familiar tools
- Inference latency (5-20ms) is tolerable since it's parallel to signal processing

**IPC keeps them isolated:**

- If Python crashes (OOM, bad model), Rust keeps running
- Hot reload Python code without restarting the service
- Clear boundary makes testing and debugging easier

Current status: the IPC task supports both simulation and the real Python bridge.
`service.ipc_simulation_enabled = true` keeps MVP simulation enabled (default).
Set `service.ipc_simulation_enabled = false` to require a connected
`neurohid-ml bridge` process.

For mixed LSL publishers (for example Emotiv multi-stream output), NeuroHID now
classifies streams by metadata at runtime and routes them by capability.
EEG-like streams feed decoder feature extraction, while auxiliary streams
(quality/metrics/motion/control) remain connected and observable without
crashing the service.

## Project Structure

```text
neurohid/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── neurohid-types/        # Shared type definitions
│   ├── neurohid-device/       # Device abstraction (Emotiv, mock)
│   ├── neurohid-signal/       # Signal processing pipeline
│   ├── neurohid-platform/     # Cross-platform HID emulation
│   ├── neurohid-storage/      # Secure profile storage
│   ├── neurohid-ipc/          # Rust↔Python communication
│   ├── neurohid-calibration/  # Calibration games (egui)
│   ├── neurohid-hub/          # Hub application (egui)
│   └── neurohid-core/         # Main service binary
└── python/
    └── neurohid_ml/           # Python ML components
        ├── decoder/           # RL policy (PPO)
        ├── errp/              # Error-related potential detection
        └── bridge/            # IPC client
```

## Emotiv Crates

The Emotiv publisher crates are maintained in a dedicated repository:

- <https://github.com/jmduea/emotiv-cortex-rs>
- crates.io: `emotiv-cortex-v2`, `emotiv-cortex-cli`

## Key Innovations

### Error-Related Potentials as Reward

Traditional BCI systems require explicit feedback ("Was that correct? Yes/No"). NeuroHID instead detects Error-Related Potentials (ErrPs)—brain signals automatically generated when you perceive an incorrect action. Your brain becomes the reward signal for reinforcement learning.

When you think "move left" but the cursor goes right, your anterior cingulate cortex generates a characteristic ERP within 200-300ms. We detect this and use it to train the decoder, creating a closed-loop system that improves through normal use.

### Continuous Online Learning

Most BCIs are "train once, use forever." NeuroHID continuously adapts:

- Signal characteristics drift over time (electrode impedance, fatigue, attention)
- User intentions evolve as they develop new interaction patterns
- The decoder improves as it gathers more examples of your brain signals

### Zero Integration Required

Applications don't know NeuroHID exists. They receive standard HID events—mouse moves, clicks, keystrokes—indistinguishable from physical input devices. This means NeuroHID works with every application, game, and operating system feature without modification.

## Getting Started

### Prerequisites

**Hardware:**

- Emotiv Insight (5-channel EEG headset)
- Emotiv Cortex service installed and running

**Software:**

- Rust 1.85+
- Python 3.12+
- PyTorch 2.10+

### Building

```bash
# Clone the repository
git clone https://github.com/jmduea/neurohid
cd neurohid

# Build the Rust components
cargo build --release

# Set up Python environment
cd python
uv sync
```

### Running

```bash
# Run the full neurohid hub
cargo run --release -p neurohid

# Start the background service
cargo run --release -p neurohid --bin neurohid-service

# (Windows) install and manage as a real Windows service
cargo run --release -p neurohid --bin neurohid-service -- --service-command install
cargo run --release -p neurohid --bin neurohid-service -- --service-command start
cargo run --release -p neurohid --bin neurohid-service -- --service-command status
cargo run --release -p neurohid --bin neurohid-service -- --service-command stop
cargo run --release -p neurohid --bin neurohid-service -- --service-command uninstall

# Optional: expose a localhost JSON control endpoint
cargo run --release -p neurohid --bin neurohid-service -- --control-port 47801
```

On Linux/macOS, named-pipe transports are unavailable. Use TCP loopback for both control and
ML bridge endpoints:

```toml
[service]
control_transport = "tcp_loopback"
control_port = 47385
ml_transport = "tcp_loopback"
ipc_port = 47384
```

Then run the Python bridge with:

```bash
uv run --directory python neurohid-ml bridge --transport tcp_loopback --port 47384
```

### Tracing and Data-Flow Observability

Runtime binaries (`neurohid`, `neurohid-service`) now emit structured `tracing` logs with
low-overhead defaults for hot paths.

- Default format: JSON (`NEUROHID_LOG_FORMAT=json`)
- Optional human-readable format: `NEUROHID_LOG_FORMAT=text`
- Filter levels use standard `RUST_LOG` (for example: `RUST_LOG=neurohid=debug`)

Hot-path traces are correlation-friendly and include runtime identifiers such as
`decision_id` and `stream_id` at stage boundaries (signal -> decoder -> action -> IPC),
while high-frequency detail remains debug-gated or periodically summarized.

Runtime observability sampling/rate limits are configurable via
`service.observability` in `SystemConfig` (global + per-component: `signal`,
`decoder`, `action`, `ipc`, `control`).

- `sample_ratio` controls deterministic sampling for hot-path debug events
- `info_max_per_minute` bounds gated info summaries
- `debug_max_per_second` bounds gated debug emissions

Shared taxonomy names are emitted as structured fields (`stage`, `span`, `event`) so
runtime/control logs can be grouped by stable labels across components.

### Advanced mode Jupyter IDE (managed)

The Hub now includes a Jupyter-first IDE workflow in Advanced mode.

1. Launch `neurohid`.
2. Switch to **Advanced** mode in Settings (if needed).
3. Open **Jupyter IDE** from the sidebar.
4. Click **Prepare Environment** once, then **Start Jupyter**.
5. Click **Open in Browser** and use notebooks under `python/notebooks`.

Advanced mode also exposes **Python Lab** in the sidebar for in-app notebook-style
kernel execution and bridge monitoring.

The Hub Visualization workspace supports draggable/resizable docked panes and restores your
last layout automatically across launches.

Control endpoint requests are line-delimited JSON with
`neurohid_types::control::ControlRequest` shape, for example:

```json
{"request_id":"1","command":{"type":"snapshot"}}
```

Persisted visualization UI state is stored in `UiConfig` via:

- `visualization_layout_preset`
- `visualization_pane_widgets`
- Visualization workspace uses `egui_dock` as the standard docking backend

Hub UI shell and primary controls now follow an always-on Armas-first component layer
(`Sidebar`, shared theme wrappers for input/select/toggle/slider/textarea/progress).
For migration and maintenance guidance, see `docs/ux/egui-visual-migration-cookbook.md`.

### Python ML Workflows (uv-first)

```bash
# Run bridge process
uv run --directory python neurohid-ml bridge

# Train + stage candidate for a profile from recorded sessions
uv run --directory python neurohid-ml train-profile-candidate --profile-id <PROFILE_ID>

# Run continuous trainer worker loop
uv run --directory python neurohid-ml trainer-worker --profile-id <PROFILE_ID>
```

### Validation Harness (V1 Matrix)

Use the built-in validation binary to run soak, latency, and boot-mode matrix checks:

```bash
# 24h soak with periodic forced bridge reconnects
cargo run -p neurohid --bin neurohid-validate -- soak --duration-secs 86400 --reconnect-interval-secs 120

# Full/fallback/degraded latency/resource comparison
cargo run -p neurohid --bin neurohid-validate -- latency-matrix --duration-secs-per-mode 120

# No-Python-bridge boot scenario matrix
cargo run -p neurohid --bin neurohid-validate -- boot-matrix --settle-secs 8
```

Use `--service-bin <path>` (or `NEUROHID_SERVICE_BIN`) when the validation binary cannot auto-locate
`neurohid-service`.

### Automation Harness Commands

Run canonical local quality gates (same script family used by CI):

```bash
# Focused Rust + docs/protocol/unsafe policy checks
pwsh -File ./.github/scripts/run-agent-ready-tasks.ps1 -RustScope focused -WithDocs -WithProtocol -WithUnsafe

# Python-only quality gates
pwsh -File ./.github/scripts/run-agent-ready-tasks.ps1 -SkipRust -WithPython

# Generate architecture index used by architecture gate
pwsh -File ./.github/scripts/generate-architecture-index.ps1
```

Impact-aware routing inputs for automation are defined in:

- `.github/scripts/classify-impact.ps1`
- `.github/automation/scope-map.json`

### Branch and Release Automation Policy

- `main` is PR-only: direct pushes are blocked by `.github/workflows/branch-policy.yml`.
- Tag workflow `.github/workflows/release.yml` verifies pre-release quality on `v*` tags.
- crates.io publishing is intentionally separated into manual workflow `.github/workflows/publish-crates.yml`.
- Repository-admin setup checklist: `docs/automation/branch-protection-checklist.md`.

## Development Roadmap

The current implementation roadmap is tracked in repository issues and milestones.

### Phase 1 (Weeks 1-3): Foundation

- Emotiv Cortex adapter
- Cross-platform HID emission
- Signal processing pipeline

### Phase 2 (Weeks 4-7): Core Infrastructure

- ErrP detection and calibration
- IPC layer
- Profile storage

### Phase 3 (Weeks 8-10): ML Integration

- Decoder (PPO policy network)
- Online training loop
- Reward integration

### Phase 4 (Weeks 11-14): Calibration & Polish

- Calibration games (Grid Maze, Target Tracking)
- First-run wizard
- System tray app

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

Areas where help is especially appreciated:

- Platform-specific testing (especially macOS)
- ErrP detection algorithm improvements
- Alternative device support (OpenBCI, Muse)
- Documentation and tutorials
- User experience design and feedback

## License

This project is dual-licensed under MIT or Apache 2.0, your choice.

## Acknowledgments

This project builds on research in brain-computer interfaces, error-related potentials, and reinforcement learning from human feedback. Key papers that informed the design:

- Chavarriaga et al. (2014) - ErrP detection in continuous control
- Iturrate et al. (2015) - Teaching brain-machine interfaces as alternative feedback
- Kreilinger et al. (2012) - Single vs. combined ErrP classification
- Pan K, Li L, Zhang L, Li S, Yang Z and Guo Y (2022) A Noninvasive BCI System for 2D Cursor Control Using a Spectral-Temporal Long Short-Term Memory Network. Front. Comput. Neurosci. 16:799019. doi: 10.3389/fncom.2022.799019
- Dylan Forenzo, Hao Zhu, Jenn Shanahan, Jaehyun Lim, Bin He, Continuous tracking using deep learning-based decoding for noninvasive brain–computer interface, PNAS Nexus, Volume 3, Issue 4, April 2024, pgae145, <https://doi.org/10.1093/pnasnexus/pgae145>

## FAQ

**Q: Does this actually work?**

A: The core technology (ErrP-based BCI) has been demonstrated in research settings. Consumer-grade hardware introduces challenges, but recent work shows 70-80%+ ErrP detection accuracy is achievable with devices like Emotiv. The main unknowns are around user experience and long-term adaptation.

**Q: How long until I can use it productively?**

A: Initial calibration takes 15-30 minutes. Basic control (moving a cursor, clicking) should work immediately after calibration. Smooth, reliable control develops over hours to days of use as the system learns your specific brain patterns.

**Q: Will it work for everyone?**

A: BCI performance varies significantly between individuals. Some people have easily detectable ErrPs; others don't. The system includes calibration quality metrics to tell you if your signals are usable.

**Q: Is my brain data private?**

A: Yes. All processing happens locally. Brain signals never leave your computer. Profile data is encrypted at rest using platform-native secure storage.
