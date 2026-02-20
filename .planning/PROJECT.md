# NeuroHID

## What This Is

NeuroHID is a suite of tools for working with biosignals (currently focused on EEG) that provides a coherent path from device discovery and signal acquisition through preprocessing, decoding, and action output. It addresses the lack of a standard, easy way to go from "EEG device in hand" to "trained decoder driving actions" by offering SDK + CLI + shared formats, an IDE-like Hub for power users and developers, and a standalone runtime for running decoders in the background. The design explicitly allows other biosignal device types and multiple output types (HID, game input, MIDI, custom) so components can be mix-and-matched for flexibility and experimentation.

## Core Value

A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.

## Requirements

### Validated

- ✓ Device discovery and connection — DeviceProvider/Device traits; LSL, Mock, Serial, BrainFlow backends
- ✓ Signal acquisition and pipeline — DeviceTask → SignalTask; filter/feature pipeline in neurohid-signal
- ✓ Decoder inference in runtime — DecoderTask, tract-onnx, ONNX models; decisions fed to ActionTask
- ✓ HID action output — ActionTask → OutletTask; neurohid-platform (enigo) for keyboard/mouse
- ✓ Rust↔Python IPC — control, trainer stream, runtime events; local transport (named pipe / TCP loopback)
- ✓ Python ML bridge — decoder, ErrP, trainer workflows, CLI (neurohid-ml)
- ✓ Hub GUI — eframe/egui; screens: dashboard, devices, profiles, calibration, visualization, python_lab, jupyter_ide, settings
- ✓ Headless runtime — neurohid-service binary; RuntimeBuilder, NeuroHidService; Windows service support
- ✓ Config and profile storage — neurohid-storage; encrypted persistence, OS keyring for secrets
- ✓ Calibration — neurohid-calibration wizard/games
- ✓ SDK facade — neurohid-sdk re-exports for embedders
- ✓ Validation harness — neurohid-validate (Soak, LatencyMatrix, BootMatrix)

### Active

- [ ] NeuroHID Hub as IDE-like experience: extensive observability and interactivity for setting up experiments and training novel decoders
- [ ] Standalone runtime experience: run in background with attached decoder; device streams drive actions via that decoder
- [ ] Coherent "standard path" from device setup to decoder training to action output (documentation, defaults, polish)
- [ ] Composable building blocks: SDK + CLI + shared formats so underlying components can be mix-and-matched by others
- [ ] Extensibility: design and APIs that allow other biosignal devices (beyond EEG) and other output types (beyond HID — e.g. game input, MIDI, custom)

### Out of Scope

- Cloud authentication or hosted services as required path — local-first
- Mobile app as v1 surface — desktop and headless first
- General end-consumer product before power-user/developer tooling is solid

## Context

- The project emerged from the author's own pain: researching how to connect an EEG device to a PC and train decoders to translate signals to actions revealed no standard or easy answer; tooling required piecing together many separate libraries and components.
- The repo is a Rust/Python monorepo: Rust runtime (device/signal/action stack, IPC, GUI, SDK), Python ML bridge (decoder, ErrP, trainer, notebooks). Existing codebase is mapped in `.planning/codebase/` (ARCHITECTURE.md, STACK.md).
- Target users: the author first; then other developers and power users who want to experiment with biosignal-driven interfaces and need composable, observable tooling.

## Constraints

- **Tech stack:** Rust (edition 2024, 1.85+) and Python (3.12+); uv for Python. No removal of existing runtime/Hub stack.
- **Local-first:** IPC and control are local (named pipe / loopback); no mandatory cloud.
- **Compatibility:** Maintain existing binaries (neurohid GUI, neurohid-service, neurohid-validate) and Python CLI/notebook entrypoints.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Hub + standalone runtime as distinct surfaces | Hub for experiment/decoder workflow and observability; runtime for "decoder in the loop" daily use without Hub open | — Pending |
| Design for multiple biosignal device types and output types | Avoid lock-in to EEG-only and HID-only; enable mix-and-match and future expansion | — Pending |
| Same product (NeuroHID) for suite, Hub, and runtime | One coherent product rather than separate projects | — Pending |

---
*Last updated: 2026-02-20 after initialization*
