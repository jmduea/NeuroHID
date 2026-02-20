# Project Research Summary

**Project:** NeuroHID
**Domain:** Biosignal/EEG developer tooling (BCI toolkits, SDKs, IDEs, runtimes)
**Researched:** 2026-02-20
**Confidence:** MEDIUM

## Executive Summary

NeuroHID is biosignal/EEG developer tooling: a hybrid Rust/Python stack that turns EEG (and related biosignals) into a coherent path from device → decoder → action (e.g. HID). Experts build this as a **linear pipeline** (acquisition → preprocessing → decoding → output) with optional **streaming middleware (LSL)** and an **IDE-like Hub** for setup, calibration, and training, plus a **standalone headless runtime** for “run in background.” The recommended approach is: keep the existing Rust runtime (device/signal/decoder/action tasks) and Python ML bridge; adopt MNE-Python, LSL, BrainFlow, and ONNX/tract-onnx as the standard stack; deliver Hub-as-IDE, standalone runtime, composable SDK/CLI/formats, and a documented “standard path” as v1; design device and output as **traits from the start** to avoid enum-based extensibility debt. Key risks: LSL “latest sample” and clock semantics, Hub–runtime coupling, format/schema evolution without versioning, and latency measured only as mean. Mitigate by centralizing stream consumption (drain-then-last), strict headless vs Hub boundary, versioned persisted formats, and percentile latency validation (p95/p99).

## Key Findings

### Recommended Stack

Stack research (see [STACK.md](.planning/research/STACK.md)) recommends a Python-side ML/analysis stack (MNE-Python 1.11+, pylsl 1.18+, BrainFlow 5.20+, PyTorch, ONNX/onnxruntime, pyxdf) and a Rust runtime path (tract-onnx, workspace-patched lsl-sys). Use **uv** for Python; Python ≥3.10; numpy &lt;3 where pylsl is used. EDF/BDF/XDF and ONNX are the interchange formats. Do not use bare `python`, Conda, legacy LSL forks, Python &lt;3.10, or a custom streaming protocol.

**Core technologies:**
- **MNE-Python** — EEG/MEG/biosignal analysis, preprocessing, I/O — de facto standard; EDF/BDF/XDF support.
- **LSL (pylsl / liblsl)** — Real-time streaming and sync — standard in BCI/research; use for multi-device and recording (XDF).
- **BrainFlow** — Device SDK — uniform API across languages; use when adding devices beyond LSL-only.
- **ONNX + onnxruntime (Python) / tract-onnx (Rust)** — Model export and inference — interop between Python training and Rust deployment; single artifact, no Python in deployed runtime.
- **pyxdf** — XDF read/write — offline analysis of LSL recordings.

### Expected Features

From [FEATURES.md](.planning/research/FEATURES.md): table stakes include device discovery/connection, real-time streaming, device-agnostic API, signal processing pipeline, recording/export, real-time visualization, calibration/setup wizard, documentation/examples, and CLI/scriptable entrypoints. Differentiators: Hub-as-IDE (observability + training), standalone runtime (decoder-in-loop), single coherent path device→decoder→action, composable SDK+CLI+formats, extensibility (plug-in devices/outputs), local-first, integrated Python/notebook + Rust runtime, HID output out of the box. Anti-features: mandatory cloud, single-device lock-in, no CLI, opaque formats, GUI-only critical ops, over-promising hard real-time, monolithic non-composable app, requiring code for basic device check.

**Must have (table stakes):** Device discovery/connection, real-time streaming, device-agnostic API, signal pipeline, recording/export, real-time visualization, calibration wizard, docs/examples, CLI/scriptable — all expected by developers.

**Should have (competitive):** Hub-as-IDE, standalone runtime, coherent standard path, composable SDK/CLI/formats, calibration first-class, local-first; extensibility (devices/outputs) as P2.

**Defer (v2+):** Optional cloud, mobile/embedded targets, general end-consumer product.

MVP v1 checklist: device/streaming/signal/decoder/HID already validated; still to complete Hub-as-IDE, standalone runtime experience, coherent standard path, composable building blocks, calibration as first-class.

### Architecture Approach

From [ARCHITECTURE.md](.planning/research/ARCHITECTURE.md): standard layout is a **linear pipeline** (acquisition → preprocessing → decoding → output) with optional LSL between stages, plus **control/config** (IPC, storage) and an **IDE/Hub** and **standalone runtime** that share the **same runtime core**. Data flow is unidirectional (samples → features/epochs → decisions → actions); control is bidirectional (Hub/CLI ↔ runtime via IPC). Key patterns: device-agnostic acquisition API (trait), task-based runtime with channel boundaries, LSL as optional streaming boundary, same runtime core for Hub and standalone, IPC for control and ML bridge. Anti-patterns: pipeline coupled to UI, no device abstraction, decoder/output tied to one effector, global mutable config.

**Major components:**
1. **Acquisition** — Discover/connect devices; produce raw/time-aligned samples (or LSL inlets); abstract via single API.
2. **Preprocessing** — Filter, re-reference, features; real-time or batch; feeds decoding.
3. **Decoding / inference** — Load/run models (ONNX etc.); map features to decisions.
4. **Output / action** — Map decisions to HID/game/MIDI/custom.
5. **Control & config** — Profiles, calibration, model paths; IPC/RPC; persisted storage.
6. **Hub (IDE)** — Device setup, calibration, training, visualization, Jupyter; talks to runtime via control IPC.
7. **Standalone runtime** — Headless process, same pipeline; controlled via same IPC/CLI.

Suggested build order: types/config → acquisition → preprocessing → decoding → output → core runtime → control & IPC → storage → standalone binary → SDK/CLI/formats → Hub → extensibility.

### Critical Pitfalls

From [PITFALLS.md](.planning/research/PITFALLS.md), top pitfalls and how to avoid:

1. **LSL “latest sample” = one pull** — Use drain-then-last (or short buffer); centralize consumption; document and test “most recent sample” semantics.
2. **LSL clock vs wall clock** — Use LSL time for relative/sync only; document and centralize any wall-clock conversion; label in UI/logs.
3. **Hub and runtime sharing GUI-dependent paths** — Strict boundary: runtime has own entrypoint and zero Hub/GUI deps; same contract tests for Hub-driven and service-only.
4. **Extensibility via “one more enum”** — Model device and output as traits from the start; document extension contract; blessed backends in-tree, third party via trait.
5. **Format/schema evolution without version** — Every persisted format has version + compatibility policy; additive evolution where possible.
6. **Latency as “average” only** — Measure and gate on p50/p95/p99 and jitter; bounded buffers; define behavior when pipeline can’t keep up.
7. **Calibration/session state not tied to identity** — Bind artifacts to profile/session/device; store metadata for reproducibility; validate on load.
8. **IPC (Rust–Python) as “same process”** — Timeouts, backpressure, version handshake; small control messages; handle “Python unavailable.”

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Types, config, and contracts
**Rationale:** Everything else depends on shared types, config/profile schema, and storage interface (ARCHITECTURE build order #1).
**Delivers:** Domain types, config and profile schema, storage interface, versioned format policy.
**Addresses:** Composable SDK/CLI/formats (contracts), coherent standard path (defaults).
**Avoids:** Format/schema without version (PITFALLS); global mutable config (ARCHITECTURE anti-pattern).

### Phase 2: Pipeline and runtime core
**Rationale:** Acquisition → preprocessing → decoding → output must exist before orchestration; pipeline is already partially validated (FEATURES MVP).
**Delivers:** Device abstraction (trait), acquisition backends (LSL, mock, etc.), signal pipeline, decoder inference, output/action layer; core runtime orchestrating tasks with channels.
**Uses:** LSL, BrainFlow (if needed), tract-onnx, existing neurohid-* crates.
**Implements:** Acquisition, Preprocessing, Decoding, Output, Core runtime (ARCHITECTURE).
**Avoids:** No device abstraction; decoder/output coupled to one effector; “latest sample” wrong (centralize drain-then-last in signal path).

### Phase 3: Control, IPC, storage, standalone runtime
**Rationale:** Runtime and clients need control transport and persistence before Hub or SDK can drive the system; standalone binary is “run in background” (FEATURES).
**Delivers:** Control RPC + optional event stream, persistence for config/profiles/secrets, standalone runtime binary (no GUI deps).
**Addresses:** Standalone runtime experience, composable SDK (control contract).
**Avoids:** Hub–runtime coupling (strict headless binary); IPC without timeout/version (PITFALLS).

### Phase 4: Composable SDK, CLI, formats
**Rationale:** Public API surface, CLI, and shared formats stabilize the contract before broadening extensibility and IDE features (ARCHITECTURE).
**Delivers:** Public SDK API, CLI commands, documented formats (ONNX, profile, stream semantics), stream-consumption and timestamp semantics in docs/examples.
**Addresses:** Composable SDK/CLI/formats, coherent standard path (docs, defaults).
**Avoids:** Latest-sample and LSL clock pitfalls (docs + examples); ambiguous stream identity (resolution by name+host/serial).

### Phase 5: Hub-as-IDE
**Rationale:** Hub depends on control, storage, and optional embedded/external runtime; provides observability, calibration, training, visualization (FEATURES).
**Delivers:** Hub UI for discovery, calibration, profiles, visualization, Python lab, Jupyter; one primary workflow (device → calibrate → train → run).
**Addresses:** Hub-as-IDE, calibration first-class, real-time visualization.
**Avoids:** Hub does “everything” with no clear workbench; calibration not tied to identity (store profile/device/session + metadata).

### Phase 6: Extensibility
**Rationale:** Design from the start; implement after core pipeline and SDK are stable (ARCHITECTURE).
**Delivers:** Trait-based device and outlet contracts, registration, plugin namespace/lifecycle, docs and one example plugin in CI.
**Addresses:** Extensibility (other devices, other outputs).
**Avoids:** Enum-based “one more backend”; undefined plugin lifecycle (PITFALLS).

### Phase Ordering Rationale

- Types/config first so all phases share one schema and versioning story.
- Pipeline and runtime core second so there is a runnable device→decoder→action path and LSL/sample semantics are correct before exposing SDK.
- Control, IPC, storage, and standalone runtime third so Hub and CLI can drive the same runtime and “run in background” is first-class.
- SDK/CLI/formats fourth to lock contracts and doc semantics before adding more surfaces (Hub, plugins).
- Hub-as-IDE fifth so the primary workflow is discoverable and calibration is reproducible.
- Extensibility last so the extension contract is defined against a stable core and SDK.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (Pipeline and runtime core):** LSL chunk size and push pattern for target devices/sample rates; bad-channel handling in standard path — verify against LSL FAQ and MNE patterns.
- **Phase 5 (Hub-as-IDE):** Primary workflow and workbench layout — validate against OpenViBE/OpenBCI mental models if needed.

Phases with standard patterns (skip research-phase unless edge cases appear):
- **Phase 1 (Types/config):** Standard versioned config/schema patterns.
- **Phase 3 (Control, IPC, storage):** Local IPC and storage patterns are well documented.
- **Phase 4 (SDK/CLI/formats):** Contract design is informed by PITFALLS and FEATURES; implementation is straightforward once contracts are set.
- **Phase 6 (Extensibility):** Trait-based plugins are a known pattern; one contract and one example suffice.

## Confidence Assessment

| Area       | Confidence | Notes |
|-----------|------------|--------|
| Stack     | MEDIUM     | PyPI/official docs for versions; “standard 2025 stack” from ecosystem search, not one authoritative BCI stack doc. |
| Features  | MEDIUM     | Table stakes and differentiators inferred from BrainFlow, OpenViBE, OpenBCI, LSL, NeuraScale; competitor table is consistent. |
| Architecture | MEDIUM  | Pipeline and component boundaries align with LSL/BrainFlow/MNE and NeuroHID codebase; build order is dependency-driven. |
| Pitfalls  | MEDIUM     | LSL FAQs and Brain Products pitfalls are authoritative; Rust–Python IPC and calibration identity from multiple sources; some single-source items. |

**Overall confidence:** MEDIUM

### Gaps to Address

- **numpy &lt;3 with pylsl:** Confirm exact numpy upper bound in repo (pylsl 1.18.1) and document in python/AGENTS.md or pyproject.toml.
- **BrainFlow Rust from source:** Build and integration steps are doc-dependent; validate in CI or docs when adding BrainFlow device backend.
- **Latency targets:** Research mentions &lt;100 ms and p95/p99; define concrete targets and Soak/LatencyMatrix criteria during phase planning for standalone runtime.
- **Plugin discovery/lifecycle:** One contract is recommended; exact mechanism (dynamic lib, subprocess, in-process trait) can be decided in Phase 6 planning.

## Sources

### Primary (HIGH confidence)
- MNE-Python 1.11 — mne.tools, PyPI (deps, Python ≥3.10, numpy &lt;3).
- PyPI: pylsl 1.18.1, brainflow 5.20.1, onnx 1.20.1, onnxruntime 1.24.2 — versions and constraints.
- LSL FAQs — latest sample, lsl_local_clock(), chunk sizes, high sampling rates.
- Brain Products — LSL pitfalls and how to avoid them.

### Secondary (MEDIUM confidence)
- BrainFlow, OpenViBE, OpenBCI, NeuraScale — features and APIs (official docs).
- ARCHITECTURE.md, FEATURES.md, STACK.md, PITFALLS.md — .planning/research and .planning/codebase.
- MNE-LSL, pyxdf — streaming and XDF usage.
- BCI pipeline literature (acquisition → preprocessing → decoding → output).

### Tertiary (LOW confidence)
- Individual projects (eeg-mouse, MindDesktop); arXiv BCI/tooling papers — useful for context, not for version or API decisions.

---
*Research completed: 2026-02-20*
*Ready for roadmap: yes*
