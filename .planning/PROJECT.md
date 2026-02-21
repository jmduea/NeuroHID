# NeuroHID

## What This Is

NeuroHID is a suite of tools for working with biosignals (currently focused on EEG) that provides a coherent path from device discovery and signal acquisition through preprocessing, decoding, and action output. It addresses the lack of a standard, easy way to go from "EEG device in hand" to "trained decoder driving actions" by offering SDK + CLI + shared formats, an IDE-like Hub for power users and developers, and a standalone runtime for running decoders in the background. The design explicitly allows other biosignal device types and multiple output types (HID, game input, MIDI, custom) so components can be mix-and-matched for flexibility and experimentation.

## Core Value

A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.

## Current State

**Shipped:** v1.0 MVP (2026-02-21) — 6 phases, 20 plans. Versioned contracts and formats; standalone runtime and control; SDK/CLI for device and pipeline config; standard path and recording; Hub-as-IDE; composable pipeline with extension contracts and example outlet plugin. All 19 v1 requirements validated.

## Next Milestone Goals

- To be defined via `/gsd:new-milestone` (questioning → research → requirements → roadmap).
- Candidate areas: latency/performance (RUNT-04, RUNT-05), advanced Hub (HUB-06, HUB-07), plugin lifecycle (EXT-04).

## Requirements

### Validated

- ✓ Device discovery and connection — DeviceProvider/Device traits; LSL, Mock, Serial, BrainFlow backends
- ✓ Signal acquisition and pipeline — DeviceTask → SignalTask; filter/feature pipeline in neurohid-signal
- ✓ Decoder inference in runtime — DecoderTask, tract-onnx, ONNX models; decisions fed to ActionTask
- ✓ HID action output — ActionTask → OutletTask; neurohid-platform (enigo) for keyboard/mouse
- ✓ Rust↔Python IPC — control, trainer stream, runtime events; local transport (named pipe / TCP loopback)
- ✓ Python ML bridge — decoder, ErrP, trainer workflows, CLI (neurohid-ml)
- ✓ Hub GUI — eframe/egui; screens: dashboard, devices, profiles, calibration, visualization, python_lab, jupyter_ide, settings, extensions
- ✓ Headless runtime — neurohid-service binary; RuntimeBuilder, NeuroHidService; Windows service support
- ✓ Config and profile storage — neurohid-storage; encrypted persistence, OS keyring for secrets
- ✓ Calibration — neurohid-calibration wizard/games
- ✓ SDK facade — neurohid-sdk re-exports for embedders
- ✓ Validation harness — neurohid-validate (Soak, LatencyMatrix, BootMatrix)
- ✓ **Hub as IDE** — discover/connect, calibrate, train, visualize, one primary workflow (HUB-01–HUB-05) — v1.0
- ✓ **Standalone runtime experience** — run in background with profile+decoder; control via CLI/Hub (RUNT-01–RUNT-03) — v1.0
- ✓ **Standard path** — documented device→decoder→actions; record/export/replay (PATH-01–PATH-02) — v1.0
- ✓ **Composable building blocks** — SDK/CLI/formats; pipeline stages swappable via contracts (COMP-01–COMP-06) — v1.0
- ✓ **Extensibility** — device/outlet contracts; example plugin in CI (EXT-01–EXT-03) — v1.0

### Active

- [ ] Latency profiling and tuning; Soak/LatencyMatrix in CI (RUNT-04, RUNT-05)
- [ ] Advanced visualization and experiment templates in Hub (HUB-06)
- [ ] Richer Python/notebook integration and workbench options (HUB-07)
- [ ] Plugin discovery/lifecycle fully specified and documented (EXT-04)

### Out of Scope

- Cloud authentication or hosted services as required path — local-first
- Mobile app as v1 surface — desktop and headless first
- General end-consumer product before power-user/developer tooling is solid

## Context

- The project emerged from the author's own pain: researching how to connect an EEG device to a PC and train decoders to translate signals to actions revealed no standard or easy answer; tooling required piecing together many separate libraries and components.
- The repo is a Rust/Python monorepo: Rust runtime (device/signal/action stack, IPC, GUI, SDK), Python ML bridge (decoder, ErrP, trainer, notebooks). Codebase mapped in `.planning/codebase/` (ARCHITECTURE.md, STACK.md).
- Target users: the author first; then other developers and power users who want to experiment with biosignal-driven interfaces and need composable, observable tooling.
- **Shipped v1.0:** 6 phases, 20 plans; extension contracts in docs/extension-contracts.md; example outlet crate and CI e2e.

## Constraints

- **Tech stack:** Rust (edition 2024, 1.85+) and Python (3.12+); uv for Python. No removal of existing runtime/Hub stack.
- **Local-first:** IPC and control are local (named pipe / loopback); no mandatory cloud.
- **Compatibility:** Maintain existing binaries (neurohid GUI, neurohid-service, neurohid-validate) and Python CLI/notebook entrypoints.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Hub + standalone runtime as distinct surfaces | Hub for experiment/decoder workflow and observability; runtime for "decoder in the loop" daily use without Hub open | ✓ Shipped v1.0 |
| Design for multiple biosignal device types and output types | Avoid lock-in to EEG-only and HID-only; enable mix-and-match and future expansion | ✓ Extension contracts and example outlet v1.0 |
| Same product (NeuroHID) for suite, Hub, and runtime | One coherent product rather than separate projects | ✓ Shipped v1.0 |
| Config/profile versioning and stream semantics | Reproducibility and compatibility | ✓ config-format.md; N=2 compatibility |
| Extension identity by name; discovery path | Simple, deterministic plugin loading | ✓ config root + /extensions; DuplicateName error |
| Loaded* wrappers (libloading + Box<dyn Trait>) | Keep extension libs alive; snapshot exposes slot names | ✓ In core and snapshot |

---
*Last updated: 2026-02-21 after v1.0 milestone*
