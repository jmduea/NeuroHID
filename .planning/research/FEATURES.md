# Feature Landscape: Biosignal/EEG Developer Tooling

**Domain:** Biosignal/EEG developer tooling (BCI toolkits, SDKs, IDEs, runtimes)
**Researched:** 2026-02-20
**Confidence:** MEDIUM (ecosystem and official docs; some table-stakes inferred from multiple products)

## Feature Landscape

### Table Stakes (Users Expect These)

Features developers assume exist. Missing these = product feels incomplete or unusable for serious work.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Device discovery and connection** | Cannot do anything without a working stream from hardware or a reliable mock. | Medium | BrainFlow, OpenViBE, LSL, OpenBCI all provide this. Device-specific params (port, IP, MAC) are normal; abstraction is expected (BoardShim, generic acquisition server). |
| **Real-time or low-latency streaming** | BCI and feedback require live data; offline-only tooling blocks the main use case. | Medium | LSL is the de facto standard for synchronization and streaming. Sub-100ms latency is a stated goal in production platforms (e.g. NeuraScale). |
| **Device-agnostic or multi-device API** | Developers switch hardware; one code path across devices is table stakes in modern toolkits (BrainFlow, OpenViBE 30+ devices). | Medium | Single-board lock-in is acceptable only for device-specific GUIs; SDKs are expected to abstract board ID + params. |
| **Signal processing pipeline (filters, basic transforms)** | Raw signals are not usable for decoding; filtering and simple transforms are baseline. | Medium | BrainFlow Signal Processing API, OpenViBE boxes (epoching, CSP, xDAWN, etc.). Can be minimal (bandpass, referencing) for MVP. |
| **Recording / export to standard formats** | Reproducibility, offline analysis, and training require export (e.g. HDF5, EDF, or documented binary). | Low–Medium | OpenBCI-Stream CLI exports to HDF5; OpenViBE has offline Tracker; formats vary but “save and reload” is expected. |
| **Real-time visualization** | Developers need to verify signal quality and pipeline before trusting decoding; GUI or stream debugger is expected. | Medium | OpenBCI GUI, OpenViBE Designer, LSL viewers. Can be a simple time-series/spectrum view. |
| **Calibration or setup wizard** | Device and subject setup (impedance, channel map, baseline) is part of the workflow; without it, “it doesn’t work” is ambiguous. | Medium | Often bundled with GUI (e.g. OpenBCI GUI for signal check). Wizard or guided steps reduce support burden. |
| **Documentation and examples** | BCI tooling is complex; missing docs or runnable examples make adoption unlikely. | Low | OpenBCI, BrainFlow, OpenViBE, LSL all emphasize docs and tutorials. |
| **CLI or scriptable entrypoints** | Automation, CI, and headless use require non-GUI access (stream, record, run decoder). | Low–Medium | OpenBCI-Stream CLI, BrainFlow bindings, LSL from Python/C++; “GUI only” is not enough for developers. |

### Differentiators (Competitive Advantage)

Features that set a product apart. Not required by every user, but highly valued and align with NeuroHID’s core value.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Hub-as-IDE (observability + experiments + training)** | Single place to set up devices, run calibration, design/train decoders, and observe the full pipeline. Reduces context-switching and “glue” work. | High | OpenViBE Designer is graph-based; OpenBCI uses GUI + separate IDE. An integrated workbench (dashboard, devices, profiles, calibration, viz, python_lab, jupyter_ide) is a differentiator. |
| **Standalone runtime (background, decoder-in-loop)** | Run a trained decoder in the background without keeping the Hub open; device → signal → decoder → action as a service. | High | Common in research (custom scripts); productized headless runtime with service/daemon support is rare in open-source BCI tooling. |
| **Single coherent path: device → decoder → action** | One documented, default path from hardware setup to trained model to HID (or other) output, without stitching multiple tools. | Medium | Ecosystem is fragmented (LSL + custom decoder + custom output); “one stack, one path” is a differentiator. |
| **Composable SDK + CLI + shared formats** | Same primitives (device, signal, decoder, action) available as library, CLI, and via documented formats so others can mix-and-match. | Medium | BrainFlow has SDK + bindings; fewer products offer CLI + formats (e.g. ONNX) in one coherent story. |
| **Extensibility (other devices, other outputs)** | Pluggable device backends and output types (HID, game input, MIDI, custom) so the stack isn’t EEG-only or keyboard-only. | Medium | BrainFlow is device-agnostic; output side is often custom. Explicit abstraction for “action outlets” is a differentiator. |
| **Local-first, no mandatory cloud** | Full workflow and runtime work offline; no required cloud auth or hosted services. | Low | Aligns with privacy and lab/embedded use; contrasts with cloud-first platforms (e.g. NeuraScale). |
| **Integrated Python/notebook + Rust runtime** | Use Python/notebooks for training and experiments while the runtime stays in Rust (performance, deployability); clear IPC contract. | High | Many setups use Python end-to-end; Rust runtime + Python ML bridge with defined IPC is a distinct architecture. |
| **HID (keyboard/mouse) output out of the box** | Map decoder decisions to OS-level input (keyboard/mouse) without building a custom driver or middleware. | Medium | Research often stays at “classification result”; productized HID output (e.g. enigo-style) is a practical differentiator. |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem attractive but create cost or misalignment with “developer tooling” or local-first goals.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Mandatory cloud or hosted backend** | Ease of deployment, “single sign-on,” sync. | Conflicts with local-first; lock-in and privacy concerns for labs/developers. | Optional cloud for backup/collab; default path stays local. |
| **Single-device or single-vendor lock-in** | Simpler support, tighter optimization. | Developers expect to swap hardware; lock-in limits adoption. | Device-agnostic API + multiple backends (LSL, BrainFlow, Mock, Serial). |
| **No CLI or scriptable API** | Simpler product surface. | Blocks automation, headless use, and integration with other tools. | Always provide CLI and/or library entrypoints alongside GUI. |
| **Opaque or proprietary data/model formats** | Perceived IP or “stickiness.” | Prevents reproducibility, tool reuse, and ONNX-style decoder portability. | Open, documented formats (e.g. ONNX for models; standard or documented for recordings). |
| **GUI-only for critical operations** | “Easier” for some users. | Power users and pipelines need scriptable flows. | GUI for exploration; same operations available via CLI/SDK. |
| **Real-time everything by default** | “BCI must be real-time.” | Can force premature optimization and complexity (e.g. hard real-time guarantees) before product fit. | Target “low enough latency” (e.g. &lt;100ms) and measure; avoid over-promising hard real-time. |
| **All-in-one monolithic app with no composability** | One install, one UX. | Prevents embedding, custom pipelines, and reuse in other tools. | SDK + CLI + formats so the stack can be composed or embedded. |
| **Requiring coding for basic device check** | “Developers can code.” | First step is “see my signals”; forcing code for that loses users. | Simple GUI or CLI to stream/visualize without writing code. |

## Feature Dependencies

```
Device discovery/connection
    └──requires──> Real-time streaming (or mock stream)
                        └──enables──> Recording/export, Real-time visualization

Signal processing pipeline
    └──requires──> Streaming (samples in)
    └──enables──> Decoder inference

Decoder inference
    └──requires──> Signal pipeline (or features), Trained model (e.g. ONNX)
    └──enables──> Standalone runtime, HID output

Calibration/setup wizard
    └──enhances──> Device connection, Decoder training (better baselines)

Hub-as-IDE
    └──requires──> Device discovery, Visualization, (optional) Python/notebook integration
    └──enhances──> Single coherent path, Composable SDK (same primitives in GUI)

Standalone runtime
    └──requires──> Device → signal → decoder → action pipeline, Config/profile storage
    └──enhances──> Single coherent path (use same decoder as in Hub)

Composable SDK + CLI + formats
    └──requires──> Documented APIs and formats
    └──enhances──> Extensibility (other devices/outputs)

Extensibility (other devices/outputs)
    └──requires──> Device-agnostic API, Abstract action/outlet layer
```

### Dependency Notes

- **Decoder inference** depends on **signal pipeline** (or precomputed features) and a **trained model**; recording/export supports offline training that produces that model.
- **Standalone runtime** is only meaningful once the pipeline (device → signal → decoder → action) exists and can be configured (e.g. profile, model path).
- **Hub-as-IDE** and **standalone runtime** share the same pipeline and config; they are two surfaces (interactive vs headless), not two stacks.
- **Composable SDK/CLI/formats** enable **extensibility**: new device backends and new output types plug in without changing core orchestration.

## MVP Definition

### Launch With (v1)

Minimum viable product for “coherent path from device to action” and “usable by developers.”

- [x] **Device discovery and connection** — Already validated (LSL, Mock, Serial, BrainFlow).
- [x] **Real-time streaming and signal pipeline** — Already validated (DeviceTask → SignalTask, filters/features).
- [x] **Decoder inference and HID output** — Already validated (DecoderTask, tract-onnx, ActionTask/OutletTask).
- [ ] **Hub-as-IDE experience** — Extensive observability and interactivity for setup, calibration, and training (in progress).
- [ ] **Standalone runtime experience** — Run in background with attached decoder; device streams drive actions (in progress).
- [ ] **Coherent “standard path”** — Documented defaults and one clear path: device setup → calibration → training → action output.
- [ ] **Composable building blocks** — SDK + CLI + shared formats so components can be mixed by others.
- [ ] **Calibration** — Wizard/games (validated in codebase); ensure they are first-class in the standard path.

### Add After Validation (v1.x)

- [ ] **Extensibility** — Clear APIs/plugins for other biosignal devices and other output types (game, MIDI, custom).
- [ ] **Richer Hub workbench** — Advanced visualization, experiment templates, better Python/notebook integration.
- [ ] **Performance and latency** — Profiling, tuning, optional latency guarantees where feasible.

### Future Consideration (v2+)

- [ ] **Optional cloud** — Backup, sync, or collaboration (only if local-first remains the default).
- [ ] **Mobile or embedded targets** — Desktop and headless first per PROJECT.md.
- [ ] **General end-consumer product** — After power-user/developer tooling is solid.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Device discovery/connection | HIGH | Done | P1 |
| Real-time streaming | HIGH | Done | P1 |
| Signal processing pipeline | HIGH | Done | P1 |
| Decoder inference + HID output | HIGH | Done | P1 |
| Recording/export | HIGH | Low–Med | P1 |
| Real-time visualization | HIGH | Medium | P1 |
| Calibration wizard | HIGH | Done (polish) | P1 |
| Hub-as-IDE (observability, training) | HIGH | High | P1 |
| Standalone runtime | HIGH | Medium | P1 |
| Coherent standard path (docs, defaults) | HIGH | Medium | P1 |
| Composable SDK + CLI + formats | HIGH | Medium | P1 |
| Documentation and examples | HIGH | Low | P1 |
| Extensibility (devices/outputs) | MEDIUM | Medium | P2 |
| Local-first, no mandatory cloud | HIGH | Done (constraint) | P1 |

**Priority key:** P1 = must have for launch; P2 = should have when possible.

## Competitor Feature Analysis

| Feature | BrainFlow | OpenViBE | OpenBCI (GUI + ecosystem) | NeuraScale | NeuroHID approach |
|---------|-----------|----------|---------------------------|------------|-------------------|
| Device-agnostic API | Yes (BoardShim, 65+ boards) | Yes (30+ devices, acquisition server) | Focus on OpenBCI hardware; BrainFlow for multi-device | Yes (30+ devices, LSL) | DeviceProvider/Device traits; LSL, Mock, Serial, BrainFlow |
| Real-time streaming | Yes | Yes | Yes (GUI, BrainFlow) | Yes (LSL, &lt;100ms) | DeviceTask → SignalTask; LSL primary |
| Signal processing | Yes (filters, ML API) | Yes (boxes: CSP, xDAWN, etc.) | Via BrainFlow / external | Yes (pipeline) | neurohid-signal filter/feature pipeline |
| IDE / workbench | No (library) | Yes (Designer, graph-based) | GUI for viz + record; dev in separate IDE | Cloud platform, APIs | Hub-as-IDE: dashboard, devices, profiles, calibration, viz, python_lab, jupyter_ide |
| Standalone runtime | No (you build it) | Run scenarios (not headless service) | No | Cloud runtime | neurohid-service; headless, config-driven |
| CLI / scriptable | Bindings only | Scripting (Lua, Python, MATLAB) | OpenBCI-Stream CLI (e.g. HDF5 export) | REST/GraphQL/gRPC | neurohid-ml CLI; SDK for embedders |
| Export formats | App-dependent | Tracker, files | HDF5 (OpenBCI-Stream) | Platform storage | Documented formats; ONNX for models |
| HID/output | No | Demos (game-like) | Community projects (e.g. eeg-mouse) | Application-level | ActionTask → OutletTask (enigo); first-class HID |
| Local-first | Yes | Yes | Yes | No (GCP) | Yes (local IPC, no required cloud) |

## Sources

- BrainFlow: [Features](https://brainflow.org/features/), [User API](https://brainflow.readthedocs.io/en/stable/UserAPI.html), [ONNX integration](https://brainflow.org/2022-09-08-onnx-tf/) (MEDIUM confidence — official).
- OpenViBE: [Features](https://openvibe.inria.fr/features), [Designer](https://openvibe.inria.fr/designer) (MEDIUM confidence — official).
- OpenBCI: [For Developers](https://docs.openbci.com/ForDevelopers/ForDevelopersLanding/), [OpenBCI GUI](https://docs.openbci.com/Software/OpenBCISoftware/GUIDocs/), OpenBCI-Stream CLI / HDF5 (MEDIUM confidence — official docs).
- Lab Streaming Layer: [Developer guide](https://labstreaminglayer.readthedocs.io/dev/dev_guide.html), [App development](https://labstreaminglayer.readthedocs.io/dev/app_dev.html) (MEDIUM confidence — official).
- NeuraScale: [Introduction](https://docs.neurascale.io/) — latency, device support, APIs (MEDIUM confidence — official).
- BCI tooling fragmentation and pitfalls: arXiv 2506.16168 (AI for EEG-based BCI), EEGUnity/transformer BCI limitations (LOW confidence — research papers, not product docs).
- EEG to HID control: eeg-mouse, GPAC (Graz), MindDesktop (LOW confidence — individual projects).

---
*Feature research for: Biosignal/EEG developer tooling (NeuroHID)*  
*Researched: 2026-02-20*
