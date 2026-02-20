# Architecture Patterns

**Domain:** Biosignal / EEG developer tooling
**Researched:** 2026-02-20
**Confidence:** MEDIUM (ecosystem patterns from LSL/BrainFlow/MNE/review literature; NeuroHID alignment from codebase)

## Standard Architecture

### System Overview

Biosignal/EEG developer tooling systems are typically structured as a **linear pipeline** from hardware to output, with an optional **orchestration/IDE layer** for development and a **standalone runtime** for deployment. Streaming middleware (e.g. LSL) often sits between acquisition and processing as a pub/sub boundary.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                    Development / IDE layer (optional)                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ Device UI   │  │ Calibration  │  │ Trainer /   │  │ Visualization /        │  │
│  │ & discovery │  │ & profiles  │  │ Lab / IDE   │  │ Jupyter / observability │  │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └───────────┬─────────────┘  │
├─────────┴────────────────┴────────────────┴────────────────────┴─────────────────┤
│                         Control & config (IPC / RPC / storage)                     │
├───────────────────────────────────────────────────────────────────────────────────┤
│                         Runtime pipeline (signal → action)                          │
├───────────────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    │
│  │ Acquisition  │───▶│ Preprocessing │───▶│ Decoding /   │───▶│ Output /      │    │
│  │ (device /     │    │ (filter,      │    │ inference    │    │ action        │    │
│  │  LSL inlet)  │    │  features)    │    │ (ONNX etc.)  │    │ (HID / custom)│    │
│  └──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘    │
├───────────────────────────────────────────────────────────────────────────────────┤
│  Optional: streaming middleware (LSL outlet/inlet, resolver) between components    │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|----------------|-------------------|
| **Acquisition** | Discover devices, connect, produce raw/time-aligned samples (or consume LSL streams). Abstract hardware via a single API (e.g. BrainFlow BoardShim, LSL inlets). | Preprocessing (samples downstream); optional IDE (discovery/connection status); config/storage (device settings). |
| **Preprocessing** | Filter, re-reference, artifact handling, feature extraction. May be real-time (ring buffer / chunk-based) or batch. | Acquisition (ingest samples); Decoding (emit feature vectors or epochs); optional IDE (live viz). |
| **Decoding / inference** | Map features (or raw windows) to decisions (classes, continuous control). Load and run models (ONNX, sklearn-style pipelines). | Preprocessing (features in); Output (decisions out); optional ML bridge (training, model updates). |
| **Output / action** | Translate decisions into effector commands: HID (keyboard/mouse), game input, MIDI, or custom. | Decoding (decisions in); platform layer (enigo, etc.); optional IDE (outlet status). |
| **Streaming middleware (optional)** | LSL: outlets (producers), inlets (consumers), resolver (discovery). Decouples acquisition from processing across processes/machines; provides sync and XDF recording. | Acquisition (outlet from device); Preprocessing (inlet); external recorders (Lab Recorder). |
| **Control & config** | Profiles, calibration results, model paths, enable/disable output. Persisted storage; RPC or IPC for runtime control. | All pipeline components (config); IDE (snapshots, commands); standalone runtime (same protocol). |
| **IDE / Hub** | Device setup, calibration, training workflows, visualization, Jupyter/lab, observability. Talks to runtime via control IPC; may embed or launch runtime. | Control (requests/snapshots); optional direct viz from pipeline (e.g. stream tap). |
| **Standalone runtime** | Headless process running the pipeline (device → signal → decoder → action). Same core as “runtime” above; no GUI. Controlled via same IPC/CLI. | Control (config, commands); pipeline components (internally). |

### Data Flow

**Primary (runtime) — unidirectional:**

```
[Device/LSL] → raw samples → [Preprocessing] → features/epochs → [Decoder] → decisions → [Output] → HID/custom
```

- **Acquisition → Preprocessing:** Stream of samples (chunks or sample-by-sample). Typed by channel count, sample rate, optional LSL timestamps.
- **Preprocessing → Decoding:** Feature vectors or epoched data at fixed intervals or event-driven.
- **Decoding → Output:** Discrete or continuous decisions (e.g. class id, confidence, regression value).
- **Output → effector:** Platform-specific actions (key press, mouse move, etc.).

**Control (bidirectional):**

```
[Hub / CLI] ←→ IPC/RPC ←→ [Runtime]  ;  [Runtime] ←→ [Storage] (config, profiles, secrets)
```

- Clients send control requests (snapshot, set_output_enabled, load_profile, etc.).
- Runtime responds with snapshots (state, integrity, telemetry) and applies commands.
- Config and profiles are read at startup and on demand; calibration/model paths come from storage.

**ML bridge (optional, often out-of-band):**

```
[Runtime] → events/telemetry → [Python bridge] → training/ErrP → model/feedback → [Runtime]
```

- Runtime streams decision events and telemetry to Python for training or ErrP.
- Python returns model artifacts or feedback; runtime loads new models or adjusts behavior via control/config.

### Suggested Build Order (for roadmap)

Dependencies between components imply this order:

1. **Types and config** — Shared domain types, config and profile schema, storage interface. No runtime behavior; everything else depends on this.
2. **Acquisition** — Device abstraction and at least one backend (e.g. LSL, mock, or BrainFlow). Enables “signal in” for the rest of the pipeline.
3. **Preprocessing** — Filter/feature pipeline consuming acquisition output. Depends on types and acquisition.
4. **Decoding** — Load and run models; consume preprocessing output, emit decisions. Depends on types and preprocessing.
5. **Output** — Map decisions to actions; platform layer (HID, etc.). Depends on types and decoding.
6. **Core runtime** — Orchestrate tasks (device → signal → decoder → action → outlet); channels and supervisor. Depends on all pipeline components.
7. **Control & IPC** — Transport and protocol for control RPC and optional event stream. Runtime and clients depend on it.
8. **Storage** — Persistence for config, profiles, secrets. Runtime and Hub depend on it.
9. **Standalone runtime binary** — Thin wrapper around core runtime + control + storage; no GUI. Enables “run in background” use case.
10. **SDK / CLI / formats** — Public API surface, CLI commands, shared file formats (e.g. ONNX, profile schema). Can be incremental once runtime exists.
11. **Hub (IDE-like)** — GUI for discovery, calibration, profiles, visualization, Python lab, Jupyter. Depends on control, storage, and optional embedded/external runtime.
12. **Extensibility** — Pluggable device backends, output types, and (optionally) middleware. Design from the start; implement after core pipeline and SDK are stable.

Rationale: Pipeline stages must exist before orchestration; orchestration and control must exist before a standalone runtime and Hub; SDK and formats stabilize the contract before broadening extensibility and IDE features.

## Architectural Patterns

### Pattern 1: Device-agnostic acquisition API

**What:** Single trait or interface (e.g. `Device` / `DeviceProvider`, BrainFlow’s `BoardShim`) for discovery, connect, start/stop stream, and read samples. Backends (LSL, Serial, BrainFlow, Mock) implement the same interface.

**When to use:** Any multi-device or multi-backend support; research and product both benefit from swapping hardware without changing pipeline code.

**Trade-offs:** Pro: portable apps, testable with mocks. Con: abstraction can hide backend-specific options; may need extension points for device-specific config.

### Pattern 2: Task-based runtime with channel boundaries

**What:** Each pipeline stage runs as a concurrent task (or thread). Stages communicate via bounded channels (samples, features, decisions). A supervisor monitors task health and triggers shutdown on critical failure.

**When to use:** Real-time pipelines where latency and backpressure matter; when you want clear boundaries and testable units.

**Trade-offs:** Pro: backpressure, clear data flow, isolated failure. Con: more moving parts; channel types and buffer sizes must be tuned.

### Pattern 3: LSL as optional streaming boundary

**What:** Use LSL outlets at the acquisition edge (or after preprocessing) and inlets in the next stage. Resolver for discovery. Enables multi-process or multi-machine pipelines and standard tools (Lab Recorder, XDF).

**When to use:** When you need sync across streams, recording to XDF, or decoupling acquisition from processing across processes. Not required if a single-process pipeline is enough.

**Trade-offs:** Pro: interoperability, sync, ecosystem tools. Con: extra process/network; latency and setup complexity.

### Pattern 4: Same runtime core for Hub and standalone

**What:** One “runtime” (orchestration + pipeline + control) used by both the IDE-like Hub (embedded or launched) and the headless service. Only the entrypoint and UI differ.

**When to use:** When you want “develop and observe in Hub” and “run in background without Hub” without maintaining two pipelines.

**Trade-offs:** Pro: single code path, consistent behavior. Con: runtime must be testable and controllable without GUI.

### Pattern 5: IPC for control and ML bridge

**What:** Local transport (named pipe, loopback TCP) for control RPC (snapshot, commands) and optional event stream (decisions, telemetry) to a Python (or other) process for training/ErrP.

**When to use:** When ML/training lives in Python and runtime in Rust/C++; when you need a single control plane for both Hub and CLI.

**Trade-offs:** Pro: language-agnostic boundary, clear protocol. Con: versioning and serialization discipline; reconnect and fallback behavior required.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Tight coupling of pipeline stages to UI

**What people do:** Hard-wire pipeline logic inside the IDE or Hub so that “run in background” requires a different code path.

**Why it's wrong:** Duplication, drift, and bugs between “run from Hub” and “run as service.”

**Do this instead:** Implement pipeline and control in a core runtime; Hub and service are two entrypoints that build and drive the same runtime via control IPC.

### Anti-Pattern 2: No device abstraction

**What people do:** Write acquisition code for one device or driver; add more devices by copy-pasting and branching.

**Why it's wrong:** Unmaintainable; swapping devices or adding mocks is costly.

**Do this instead:** Introduce a device/provider abstraction and implement backends (LSL, BrainFlow, mock, etc.) behind it; keep pipeline code device-agnostic.

### Anti-Pattern 3: Decoding and output coupled to a single effector type

**What people do:** Decoder output is “keyboard only” or “one game API”; adding MIDI or custom output requires changing decoder or core.

**Why it's wrong:** Limits extensibility and forces rewrites for new output types.

**Do this instead:** Decoder emits generic “decisions”; an output/action layer maps decisions to one or more effector types (HID, game, MIDI, custom) via a small adapter interface.

### Anti-Pattern 4: Global mutable state for pipeline config

**What people do:** Global variables or singletons for device selection, model path, output enabled.

**Why it's wrong:** Hard to test, race-prone, and unclear ownership when multiple clients (Hub, CLI) control the runtime.

**Do this instead:** Explicit config and profile objects; control RPC to mutate state; runtime holds a single source of truth and responds with snapshots.

## Integration Points

### External / ecosystem

| Service / system | Integration pattern | Notes |
|------------------|---------------------|--------|
| LSL (liblsl) | Inlets in acquisition or after device driver; optional outlets for recording | Use resolver for discovery; respect chunk size and buffer for latency. |
| BrainFlow | Implement acquisition backend behind device trait; call BoardShim prepare_session/start_stream/get_board_data | Same process; no LSL required unless you also stream to LSL. |
| MNE / MNE-LSL | Python side: StreamLSL for intake; optional preprocessing/decoding in Python; IPC or LSL back to runtime if needed | MNE-realtime deprecated for LSL; prefer mne-lsl. |
| Lab Recorder / XDF | Consume LSL streams; no direct API from runtime | Run as separate process; ensure stream metadata (channel count, sfreq) is correct. |

### Internal boundaries

| Boundary | Communication | Notes |
|----------|---------------|--------|
| Acquisition ↔ Preprocessing | Samples (in-process channels or LSL) | Define sample type and chunk size once; keep timestamps if needed for sync. |
| Preprocessing ↔ Decoding | Feature vectors or epochs | Fixed schema so decoder and trainer agree; version model input schema. |
| Decoding ↔ Output | Decisions (e.g. class, confidence, value) | Keep decision type generic; output layer does effector-specific mapping. |
| Runtime ↔ Hub / CLI | IPC (control RPC + optional event stream) | Version protocol; handle reconnect and backward compatibility. |
| Runtime ↔ Python bridge | IPC (events to Python; model/feedback from Python) | Same transport as control or separate channel; define envelope format. |
| Runtime ↔ Storage | Read/write config, profiles, secrets | Encrypt sensitive data; use OS keyring for keys; avoid storing raw credentials in config. |

## Scalability Considerations

| Scale | Architecture adjustments |
|-------|---------------------------|
| Single user, one device | Single process runtime; optional LSL only for recording or external tools. |
| Single user, multiple streams | LSL or multi-backend acquisition; one pipeline per “profile” or merged streams; keep decoder/output per use case. |
| Many users (e.g. lab) | Typically multiple independent runtimes (one per station); shared formats and CLI for consistency; no need for a central server unless you add one. |

For NeuroHID’s stated scope (local-first, desktop and headless, power users/developers), scaling “out” to many users is secondary; the main scaling concern is **throughput and latency** of the pipeline (sample rate, buffer sizes, decoder lag). Prefer clear boundaries and measurable latency over multi-tenant architecture.

## Sources

- LSL: [Introduction — Labstreaminglayer 1.13](https://labstreaminglayer.readthedocs.io/info/intro.html) — Stream outlet/inlet, resolver, samples/chunks, XDF.
- MNE-LSL: [Introduction to real-time LSL streams](https://mne.tools/mne-lsl/stable/generated/tutorials/00_introduction.html) — StreamLSL, PlayerLSL, ring buffer, MNE-style API.
- BrainFlow: [BrainFlow documentation](https://brainflow.readthedocs.io/en/stable/index.html) — Data Acquisition API vs Signal Processing/ML API; BoardShim; multi-language.
- BCI pipeline: Literature (e.g. acquisition → preprocessing → decoding → output) — [Signal acquisition review (ScienceDirect 2024)](https://www.sciencedirect.com/science/article/pii/S2667325824001559); [BCI decoding toolbox (Frontiers 2024)](https://www.frontiersin.org/journals/human-neuroscience/articles/10.3389/fnhum.2024.1358809/full).
- NeuroHID codebase: `.planning/codebase/ARCHITECTURE.md`, `PROJECT.md` — Layers, tasks, data flow, Hub vs service.

---
*Architecture research for: biosignal/EEG developer tooling (NeuroHID subsequent milestone: Hub-as-IDE, standalone runtime, SDK/CLI/formats, extensibility).*
*Researched: 2026-02-20*
