# Stack Research

**Project:** NeuroHID (biosignal/EEG developer tooling — subsequent milestone)
**Domain:** Biosignal/EEG developer tooling and decoder pipelines
**Researched:** 2026-02-20
**Confidence:** MEDIUM (official PyPI/docs for versions; ecosystem from WebSearch)

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|------------------|
| **MNE-Python** | 1.11.0 | EEG/MEG/biosignal analysis, preprocessing, decoding, I/O | De facto standard for neurophysiology in Python; EDF/BDF/XDF support, ML decoding tutorials, active maintenance. Python ≥3.10. |
| **Lab Streaming Layer (LSL)** | pylsl 1.18.1 | Real-time streaming and sync across devices/apps | Standard in BCI/research for multi-device, multi-app streams; PyBCI, BrainFlow, pyOpenNFT all use LSL. Use pylsl for Python; liblsl for Rust (e.g. workspace-patched lsl-sys). |
| **BrainFlow** | 5.20.1 (Python) | Device SDK for EEG/EMG/ECG acquisition | Uniform API across 9 languages (Python, Rust, C++, etc.); broad device support (OpenBCI, Muse, Mentalab, etc.). Use when adding devices beyond custom/LSL-only. |
| **ONNX + ONNX Runtime** | onnx 1.20.1, onnxruntime 1.24.2 | Model export and inference for decoders | Interop between PyTorch/TF training and Rust/Python inference; runtime is the standard deployment engine. Python ≥3.10. |
| **tract-onnx** (Rust) | 0.22 | ONNX inference in Rust runtime | Fits NeuroHID’s existing Rust decoder path; no Python runtime in deployed service. Keep aligned with ONNX opset used by Python export. |

### Decoder Pipeline (Python ML side)

| Technology | Version | Purpose | When to Use |
|------------|---------|---------|-------------|
| **PyTorch** | ≥2.10 | Training and export of decoder models | Default for custom neural decoders and ONNX export; already in NeuroHID. |
| **scikit-learn** | ≥1.8 | Classical ML (e.g. LDA, SVM) and pipelines | Lightweight decoders, calibration baselines, feature selection. |
| **scipy** | ≥1.11 | Filters, spectral/statistical helpers | Preprocessing and feature extraction in trainer/offline analysis. |
| **numpy** | ≥1.26 (MNE) / project choice | Array ops and model I/O | Core dependency; align with MNE (e.g. &lt;3 if using pylsl 1.18.1). |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|--------------|
| **pyxdf** | ≥1.17 | Read/write XDF (LSL-recorded) files | Offline analysis of LSL recordings; MNE’s XDF example uses pyxdf. |
| **pylsl** | 1.18.1 | LSL Python bindings | Any Python component that consumes or publishes LSL streams (trainer, mock devices, Hub tooling). |
| **onnx** (Python) | ≥1.19 | ONNX graph build/export/check | Export from PyTorch/sklearn to ONNX for runtime; validate shapes. |

### Data Formats

| Format | Role | Tooling |
|--------|------|---------|
| **EDF/EDF+** | Standard 16-bit EEG file format | MNE `read_raw_edf`; write via pyedflib or MNELAB if needed. |
| **BDF** | 24-bit BioSemi variant | MNE `read_raw_bdf`. |
| **XDF** | Multi-stream LSL recordings | pyxdf for load; LSL for record. Use for “record from Hub/runtime, analyze offline”. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|------|
| **uv** | Python env and package management | Project standard; use `uv run --project python` / `uv sync --directory python`. |
| **pytest** | Python tests | ≥9.0; use with pytest-cov, pytest-asyncio for async IPC tests. |
| **ruff / black / mypy** | Lint, format, type-check | Per `python/AGENTS.md`; line-length 100. |
| **JupyterLab** | Notebooks for exploration and Hub IDE | ≥4.4; fits “Hub as IDE” and composable SDK/CLI. |

## Installation

```bash
# Python (uv)
uv sync --directory python   # uses python/pyproject.toml

# Core biosignal/EEG stack (if adding to a fresh env)
uv add mne>=1.11.0 pylsl>=1.18 brainflow>=5.20 pyxdf>=1.17
uv add onnx>=1.19 onnxruntime>=1.24 torch scipy scikit-learn numpy
```

Rust: existing workspace; LSL via optional `neurohid-device` with workspace-patched `lsl-sys`. BrainFlow Rust from source after building C++ core (see BrainFlow docs).

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|--------------------------|
| MNE-Python | EEGLAB (MATLAB), FieldTrip | When team or pipeline is already MATLAB-based. |
| LSL | Custom TCP/UDP, MQTT | LSL when you need sync and ecosystem compatibility; custom when you own all endpoints and need minimal deps. |
| BrainFlow | Vendor SDKs only | BrainFlow when supporting many devices with one API; vendor SDK when one device and vendor support is required. |
| ONNX Runtime | Python-only inference (Torch) | ONNX when deploying in Rust or other non-Python runtimes, or when you want one artifact for multiple runtimes. |
| tract-onnx | onnxruntime-rs / candle | tract-onnx when already in use and opset coverage is sufficient; consider others if hitting unsupported ops. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **Bare `python` in scripts/docs** | Project policy is uv-first. | `uv run --project python` / `uv run --directory python`. |
| **Conda for this project** | Repo standard is uv; lockfile and CI are uv-based. | uv for Python. |
| **Legacy LSL forks** | Old pylsl/chkothe forks; liblsl ABI can differ. | Official labstreaminglayer/pylsl and sccn/liblsl (or patched lsl-sys in workspace). |
| **Python &lt;3.10 for new code** | MNE 1.11 and onnx/onnxruntime require ≥3.10. | Python 3.12+ per PROJECT.md. |
| **numpy 3.x with pylsl** | pylsl 1.18.1 constrains numpy to &lt; 3. | numpy &lt; 3 where pylsl is in the same env; otherwise follow MNE (numpy ≥1.26, &lt; 3). |
| **Building a custom streaming protocol** | LSL is the standard for BCI/research; reinventing fragments the ecosystem. | LSL for streaming; XDF for recorded streams. |
| **Proprietary BCI stacks as the only path** | Composable SDK/CLI/formats require open, documented interfaces. | Use open formats (ONNX, EDF/XDF) and LSL/BrainFlow/MNE as the standard path. |

## Stack Patterns by Variant

**If building Hub-as-IDE / notebooks only (no Rust runtime):**
- Use Python-only: MNE, pylsl, pyxdf, PyTorch, onnx, onnxruntime.
- Decoder can stay in Python (onnxruntime) for simplicity.

**If running decoders in standalone Rust runtime:**
- Train/export in Python (PyTorch → ONNX); run inference in Rust with tract-onnx.
- Keep ONNX opset and input/output shapes documented so Python export and Rust inference stay in sync.

**If adding non-EEG biosignals (EMG, etc.):**
- Keep MNE for EEG-heavy analysis; use BrainFlow for multi-modal device access.
- Same LSL/XDF and ONNX decoder pipeline can carry other signal types if channel semantics are defined.

**If exposing composable CLI/formats:**
- Prefer EDF/XDF and ONNX as interchange formats; document channel layouts and decoder I/O in docs.
- Use LSL stream names/types so third-party tools can discover and consume streams.

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| mne 1.11.0 | Python ≥3.10, numpy &lt;3, ≥1.26, scipy ≥1.11 | MNE PyPI deps. |
| pylsl 1.18.1 | Python ≥3.9, numpy &lt;3, ≥1.21 | On non-Windows, liblsl must be available (e.g. PYLSL_LIB or system lib). |
| brainflow 5.20.1 | numpy, setuptools (no strict version) | Precompiled wheels for x64 Win/Linux/macOS; Rust from source. |
| onnx 1.20.1 | Python ≥3.10, numpy ≥1.23.2, protobuf ≥4.25.1 | Export and graph manipulation. |
| onnxruntime 1.24.2 | Python ≥3.10, numpy ≥1.21.6 | Use one of onnxruntime or onnxruntime-gpu per env. |
| tract-onnx 0.22 | Rust, ONNX models (opset) | Verify opset compatibility with Python-exported models. |

## Confidence

| Area | Level | Reason |
|------|-------|--------|
| Core (MNE, LSL, BrainFlow) | HIGH | Versions from PyPI and official docs (mne.tools, brainflow.readthedocs.io, pypi.org/project/pylsl). |
| Decoder (ONNX, onnxruntime, tract) | HIGH | PyPI and crates.io; tract-onnx version from repo. |
| Ecosystem (PyBCI, pyxdf, formats) | MEDIUM | PyPI + MNE docs + WebSearch; not re-verified with Context7. |
| “Standard 2025 stack” claim | MEDIUM | LSL and MNE are widely cited; BrainFlow and ONNX adoption from docs and search; no single authoritative “BCI stack” document. |

## Sources

- [MNE-Python 1.11.0](https://mne.tools/stable/index.html) — homepage, capabilities
- [PyPI: mne](https://pypi.org/project/mne/) — 1.11.0, deps (Python ≥3.10, numpy &lt;3,≥1.26, scipy ≥1.11)
- [PyPI: brainflow](https://pypi.org/project/brainflow/) — 5.20.1
- [BrainFlow Installation](https://brainflow.readthedocs.io/en/stable/BuildBrainFlow.html) — Python pip, Rust from source
- [PyPI: pylsl](https://pypi.org/project/pylsl/) — 1.18.1, numpy &lt;3,≥1.21
- [PyPI: onnxruntime](https://pypi.org/project/onnxruntime/) — 1.24.2
- [PyPI: onnx](https://pypi.org/project/onnx/) — 1.20.1
- [PyPI: pyxdf](https://pypi.org/project/pyxdf/) — 1.17.0 (WebSearch)
- [.planning/codebase/STACK.md](.planning/codebase/STACK.md) — existing NeuroHID Rust/Python stack
- WebSearch: EEG/biosignal developer tooling 2025, PyBCI, LSL, decoder pipelines, EDF/XDF/MNE

---
*Stack research for: biosignal/EEG developer tooling and decoder pipelines (subsequent milestone).*
