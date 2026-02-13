# NeuroHID

**Transform consumer EEG devices into standard PC peripherals using deep reinforcement learning.**

NeuroHID is a system that learns to decode your intentions from brain signals and translates them into mouse movements, clicks, and keyboard inputs. It runs as a background service, requiring no application integration—your computer just gains a new input device that happens to be controlled by your thoughts.

## Vision

Imagine putting on a lightweight EEG headset, thinking "move left," and watching your cursor smoothly glide across the screen. No training wheels, no special applications, no steep learning curve. NeuroHID continuously learns from the implicit feedback your brain generates when actions don't match your intentions, getting better the more you use it.

## Architecture

NeuroHID uses a hybrid Rust/Python architecture:

```
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

Current status: the IPC task defaults to a simulated bridge for MVP development. Set `service.ipc_simulation_enabled = false` in your config to require a real Python bridge (the service will report IPC unavailable until that bridge is implemented).

## Project Structure

```
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

- Rust 1.75+
- Python 3.12+
- PyTorch 2.0+

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
```

### Python ML Workflows (uv-first)

```bash
# Run bridge process
uv run --directory python neurohid-ml bridge

# Train + stage candidate for a profile from recorded sessions
uv run --directory python neurohid-ml train-profile-candidate --profile-id <PROFILE_ID>

# Run continuous trainer worker loop
uv run --directory python neurohid-ml trainer-worker --profile-id <PROFILE_ID>
```

## Development Roadmap

The current implementation roadmap is tracked in repository issues and milestones.

**Phase 1 (Weeks 1-3): Foundation**

- Emotiv Cortex adapter
- Cross-platform HID emission
- Signal processing pipeline

**Phase 2 (Weeks 4-7): Core Infrastructure**

- ErrP detection and calibration
- IPC layer
- Profile storage

**Phase 3 (Weeks 8-10): ML Integration**

- Decoder (PPO policy network)
- Online training loop
- Reward integration

**Phase 4 (Weeks 11-14): Calibration & Polish**

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

This project is dual-licensed under MIT and Apache 2.0. The Emotiv crate-specific license files now live in the external `emotiv-cortex-rs` repository.

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
