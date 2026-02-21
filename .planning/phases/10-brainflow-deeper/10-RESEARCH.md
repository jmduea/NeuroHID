# Phase 10: BrainFlow Deeper — Research

**Researched:** 2026-02-21  
**Domain:** BrainFlow real C++ SDK integration, Rust bindings, build order, streaming into NeuroHID pipeline  
**Confidence:** HIGH (official docs + repo layout + codebase); MEDIUM (Rust crate build integration details)

## Summary

Phase 10 adds the **real** BrainFlow SDK behind a feature flag so developers can build and use native BrainFlow in neurohid-device while default and CI keep using the synthetic board only. The same Device → SampleStream → pipeline path used by LSL and the current BrainFlow simulation adapter will carry real BrainFlow data; no second pipeline. Reproducibility is achieved by pinning the BrainFlow C++ version and build steps and documenting them.

The codebase already has the right abstraction: `BrainFlowProvider` / `BrainFlowDevice` implement `DeviceProvider` and `Device`; `start_streaming()` returns a `SampleStream` that the device task forwards into the existing `sample_tx` channel. Today the inner implementation is `MockDevice`. Phase 10 adds a **native** implementation (using BrainFlow’s BoardShim: prepare_session → start_stream → get_board_data loop mapped to `Sample`) behind a new feature (e.g. `brainflow-native`). Default and CI do **not** enable `brainflow-native`; they keep using the existing synthetic adapter (BRAIN-06). The streaming path is already unified (BRAIN-07). Pinning and documentation satisfy BRAIN-08.

**Primary recommendation:** Add feature `brainflow-native` in neurohid-device that depends on the official BrainFlow Rust binding (git or vendored), build C++ first then Rust with libs in a documented, pinned way; implement native `Device`/`DeviceProvider` that maps BoardShim data to `Sample` and the existing `SampleStream` type; keep default and CI on synthetic only; document version and build steps in docs/brainflow.md and optionally a small build script or CI job for reproducible native builds.

---

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| BRAIN-06 | Developer can build and use the real BrainFlow SDK in neurohid-device behind a feature flag (e.g. brainflow-native); default and CI use synthetic board only (no mock) | Standard Stack: BrainFlow C++ build then Rust binding; feature `brainflow-native` gates real SDK; current `brainflow` feature stays synthetic-only. Architecture: same Provider/Device types, native implementation behind flag. |
| BRAIN-07 | User or developer can use the BrainFlow streaming path into the same signal pipeline as LSL and other backends (synthetic or real board → Device → pipeline) | Architecture: DeviceTask already uses `device.start_streaming()` → `SampleStream` → `sample_tx` → pipeline; native BrainFlow device just implements `Device` and returns a stream that yields `Sample`; no second pipeline. |
| BRAIN-08 | BrainFlow version and build steps (C++ core then Rust) are pinned and documented so builds are reproducible | Don’t Hand-Roll: use BrainFlow’s tools/build.py and rust_package; document exact version (git tag/commit), build.py args, and copy/link steps. Standard Stack: pin BrainFlow release (e.g. 5.13.0 tag), document CMake/build.py options. |

</phase_requirements>

---

## User Constraints

No CONTEXT.md exists for Phase 10. No locked decisions or deferred ideas to copy.

---

## Standard Stack

### Core

| Component | Version / source | Purpose | Why standard |
|-----------|------------------|---------|--------------|
| BrainFlow C++ core | Pinned tag/commit (e.g. 5.13.0) | BoardController, DataHandler, MLModule libs | Official SDK; no prebuilt Rust crate on crates.io; Rust binding expects built C++ and dynamic libs. |
| BrainFlow Rust binding | In-tree: `brainflow-dev/brainflow` → `rust_package/brainflow` | BoardShim API, bindgen FFI | Only official Rust support; build after C++ with `lib/` populated from C++ install. |
| neurohid-device | Existing | DeviceProvider / Device | Same abstraction; native path is a second implementation behind `brainflow-native` feature. |

### Build order (reproducible)

1. **C++:** From BrainFlow repo root: `python tools/build.py` (optionally with `--brainflow-version`, `--cmake-install-prefix`, `--build-dir`). Output: `installed/lib/` (BoardController, DataHandler, MLModule dylibs) and `installed/inc/` (headers).
2. **Rust:** The in-tree `rust_package/brainflow` crate’s `build.rs` expects a `lib/` directory **inside** the crate (next to its `Cargo.toml`), copies it to `OUT_DIR`, and links. So either: (a) copy `installed/lib/*` into `rust_package/brainflow/lib/` then `cargo build` in that crate, or (b) patch the crate to accept an env var (e.g. `BRAINFLOW_LIB_DIR`) and link from there.
3. **NeuroHID:** Build with `cargo build -p neurohid-device --features brainflow-native` (and `brainflow` if not default). Reproducibility: pin BrainFlow git ref and build.py options; document in docs.

### Feature flags

| Feature | Meaning | Default / CI |
|--------|--------|--------------|
| `brainflow` | BrainFlow (synthetic) adapter; current implementation | On in default build for Hub/examples (Phase 9). |
| `brainflow-native` | Real BrainFlow SDK (C++ + Rust binding); native Device implementation | Off by default and in CI. |

### Alternatives considered

| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| Git dep on brainflow-dev/brainflow rust_package | Vendor rust_package (submodule or copy) | Vendor gives full control and reproducible layout (copy libs into crate); git dep is simpler but requires users to supply libs or we patch build.rs. |
| Env var for lib path | Always copy installed/lib into crate’s lib/ | Env var avoids copying but requires patching upstream build.rs; copy approach works with unmodified BrainFlow repo. |

---

## Architecture Patterns

### Current device → pipeline flow (unchanged)

- `DeviceTask` holds `Box<dyn DeviceProvider>`; on Connect it calls `provider.connect(stream_id)` → `Box<dyn Device>`; then `spawn_stream_task(device, …)` calls `device.start_streaming().await` → `SampleStream`.
- `SampleStream` is consumed in a loop: `sample_stream.next()` → `Sample` → `sample_tx.send(sample)` → downstream pipeline (decoder, actions). Same for LSL, Serial, Mock, and BrainFlow synthetic.
- **BRAIN-07:** A native BrainFlow device is just another `Device` that returns a `SampleStream` yielding `Sample`; no new pipeline path.

### Native BrainFlow device implementation

- **Provider:** When `brainflow-native` is enabled, `BrainFlowProvider` (or a separate type behind the same config) uses BrainFlow’s `BoardShim` + `BrainFlowInputParams` (board_id, serial port, etc.) to discover/connect. Discovery can return real devices (e.g. from serial) or a single “native board” entry; connect builds a `BoardShim` and prepares session.
- **Device:** Implement `Device` with an inner that holds a `BoardShim` (or equivalent from the Rust binding). `start_streaming()` returns a `SampleStream` that:
  - Runs a loop (or async polling) calling `get_board_data()` (or `get_current_board_data()`).
  - Maps 2D data to `Sample`: use `get_timestamp_channel(board_id)`, `get_eeg_channels(board_id)` (or equivalent from the binding); one `Sample` per column (or batched) with `device_timestamp`, `values` (EEG row slice), `sequence_number` from package number channel if available.
  - Yields `Result<Sample>`; use `neurohid_types::now_micros()` for `system_timestamp` if needed.
- **Units:** BrainFlow returns EEG in µV; NeuroHID `Sample.values` are in µV — no conversion needed.
- Keep `normalize_metadata()`-style board metadata (channels, sampling rate) for DeviceInfo/channel_config; can derive from BrainFlow’s `get_board_descr()` / get_sampling_rate / get_eeg_channels when available in the Rust API.

### Recommended project structure (neurohid-device)

- Keep `brainflow.rs` as the single BrainFlow module. When only `brainflow` is enabled: current code (synthetic via Mock). When `brainflow-native` is also enabled: either (a) same file with `#[cfg(feature = "brainflow-native")]` blocks that use the real SDK for connect/stream, or (b) a separate `brainflow_native.rs` that implements the native path and is used by `brainflow.rs` when the feature is on. Do not re-export BrainFlow-specific types (e.g. BoardShim) in the public API; wrap everything in `DeviceProvider`/`Device` and `Sample`.

### Anti-patterns to avoid

- **Two pipelines:** Do not add a separate “BrainFlow pipeline”; the single pipeline is Device → SampleStream → sample_tx → existing consumers.
- **Default/CI with native:** Do not enable `brainflow-native` in default features or in CI; it requires C++ build and platform-specific libs.
- **Re-exporting brainflow crate types:** Keep the brainflow dependency under `brainflow-native` and do not re-export its types in neurohid-device’s public API to avoid pulling older deps (ndarray etc.) into the rest of the workspace.

---

## Don't Hand-Roll

| Problem | Don’t build | Use instead | Why |
|--------|-------------|--------------|-----|
| Board/session/stream API | Custom C++ or Rust wrapper around raw boards | BrainFlow BoardShim (via official Rust binding) | Board-specific details, presets, and data layout are already handled. |
| Data layout (rows = channels, columns = samples) | Manual interpretation of buffers | get_timestamp_channel(), get_eeg_channels(), get_board_data() | Board-specific row indices; BrainFlow documents and provides helpers. |
| C++ build and link | Custom CMake or hand-written link rules | BrainFlow’s `tools/build.py` and rust_package build.rs | Reproducible build order and link names (BoardController, DataHandler, MLModule). |
| Binding generation | Own bindgen setup for BrainFlow headers | brainflow crate’s `generate_binding` feature (and pre-generated ffi) | Headers and enum/struct layout are maintained by BrainFlow. |

**Key insight:** BrainFlow already provides a uniform C++ API and an in-tree Rust binding; the only integration work is (1) building C++ then Rust in a pinned way, and (2) mapping BoardShim’s get_board_data + channel helpers into `Sample` and `SampleStream`.

---

## Common Pitfalls

### Pitfall 1: Expecting brainflow crate on crates.io with prebuilt libs

- **What goes wrong:** Assuming `cargo add brainflow` is enough; build fails because the crate expects `lib/` with dylibs next to the crate.
- **Why:** The official Rust binding lives in the BrainFlow repo and is designed to be built after the C++ core; it does not ship prebuilt libs on crates.io.
- **How to avoid:** Document and script: build C++ first, then either copy `installed/lib` into the Rust crate’s `lib/` (if vendored) or set an env var and patch build.rs (if using git dep). See Standard Stack build order.
- **Warning signs:** Build errors like “cannot find -lBoardController” or missing lib directory.

### Pitfall 2: Enabling brainflow-native in default or CI

- **What goes wrong:** Default or CI build requires C++ toolchain and BrainFlow build; CI becomes heavy and platform-specific; users without BrainFlow built get link errors.
- **Why:** BRAIN-06 explicitly requires default and CI to use synthetic only.
- **How to avoid:** Keep `brainflow-native` off in neurohid-device and neurohid-core default features; do not add it to the feature set used by CI unless it’s a dedicated optional job that builds BrainFlow then builds with `brainflow-native`.
- **Warning signs:** CI or `cargo build` failing with BrainFlow link errors when no C++ build was run.

### Pitfall 3: Wrong data shape or channel order

- **What goes wrong:** Samples have wrong channel order or timestamps; pipeline or decoder sees inconsistent data.
- **Why:** BrainFlow returns [num_channels x num_data_points]; which row is timestamp/EEG is board-specific.
- **How to avoid:** Use the binding’s equivalents of `get_timestamp_channel(board_id)`, `get_eeg_channels(board_id)` (and package_num_channel for sequence_number if desired); map columns to `Sample` in that order. Document which preset (DEFAULT_PRESET) is used.
- **Warning signs:** Channel count mismatch in integrity tracker; nonsensical timestamps.

### Pitfall 4: Forgetting to release_session / stop_stream

- **What goes wrong:** Resource leaks or hang on disconnect.
- **Why:** BrainFlow requires stop_stream and release_session after use.
- **How to avoid:** In `Device::stop_streaming` and `Device::disconnect`, call the BrainFlow stop/release APIs; mirror the same lifecycle as prepare_session → start_stream → … → stop_stream → release_session.
- **Warning signs:** Second connect fails or process doesn’t exit cleanly.

---

## Code Examples

### BrainFlow build order (official docs)

```bash
# From BrainFlow repo root (pinned tag/commit for BRAIN-08)
python tools/build.py
# Installs to installed/ by default. For Rust:
# Copy installed/lib/* to rust_package/brainflow/lib/ then:
cd rust_package/brainflow
cargo build
# Optional: cargo build --features generate_binding to regenerate bindings after C++ header changes
```

Source: [BuildBrainFlow](https://brainflow.readthedocs.io/en/stable/BuildBrainFlow.html) (Rust section).

### BrainFlow Rust: lib location (build.rs)

The crate expects `lib/` inside the crate and links BoardController, DataHandler, MLModule:

```rust
// build.rs (brainflow crate) - expects CARGO_MANIFEST_DIR/lib/ to exist
let lib_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("lib");
// copies to OUT_DIR, then
println!("cargo:rustc-link-search=native={}/lib", out_path.display());
println!("cargo:rustc-link-lib=dylib=BoardController");
// ...
```

So for a vendored workflow: after `python tools/build.py`, copy `installed/lib/*` to `rust_package/brainflow/lib/`.

### Mapping get_board_data to Sample (conceptual)

```text
# BrainFlow: 2D array [num_rows x num_cols]; rows = channels (timestamp, EEG, ...), cols = packages
# Use get_timestamp_channel(board_id), get_eeg_channels(board_id)
# Sample: device_timestamp, system_timestamp, sequence_number, values (Vec<f32>), source_id
# Per column j: timestamp_row[j] -> device_timestamp; eeg_rows[:, j] -> values; package_num_row[j] -> sequence_number
```

Units: BrainFlow EEG in µV; Sample.values in µV — no conversion.

### Existing pipeline consumption (no change)

```rust
// neurohid-core/src/tasks/device/streaming.rs
let stream_result = device.start_streaming().await;
let mut sample_stream = stream_result?;
// ...
sample_result = sample_stream.next() => {
    if let Some(Ok(sample)) = sample_result {
        // integrity, quality, calibration, then:
        sample_tx.send(sample).await
    }
}
```

Native BrainFlow just provides a `Device` whose `start_streaming()` returns a stream that yields `Sample` in the same format.

---

## State of the Art

| Old / current | Phase 10 target | Impact |
|---------------|------------------|--------|
| BrainFlow in NeuroHID = synthetic only (Mock inside BrainFlowDevice) | Real SDK behind `brainflow-native`; same Device/Provider abstraction | Developers can use real hardware; default/CI unchanged. |
| No pinned BrainFlow version or build steps | Document and pin version + build.py + copy/link steps | Reproducible builds (BRAIN-08). |
| Single “brainflow” feature | `brainflow` (synthetic) + `brainflow-native` (real SDK) | Clear separation; no accidental native in CI. |

**Deprecated / out of scope for Phase 10:** Using a crates.io “brainflow” package that is not the official in-tree binding. Replacing the synthetic adapter entirely (synthetic stays for default/CI). Adding a second pipeline for BrainFlow.

---

## Open Questions

1. **Vendor vs git dependency**
   - What we know: BrainFlow Rust crate expects `lib/` inside the crate; C++ must be built first. STACK.md recommends git dep + user/CI builds BrainFlow and “neurohid-device’s build script finds libs” — but the brainflow crate’s build.rs owns linking and doesn’t read BRAINFLOW_DIR.
   - What’s unclear: Whether to vendor `rust_package/brainflow` and a script that builds C++ and copies libs, or depend on git and submit/patch build.rs to support e.g. BRAINFLOW_LIB_DIR.
   - Recommendation: Plan for vendored rust_package (or full repo submodule) and a documented script that runs build.py then copies `installed/lib` into the crate’s `lib/` so that `cargo build --features brainflow-native` works without patching upstream. If upstream adds env-based lib path, we can switch to git dep later.

2. **Rust crate version / git ref**
   - What we know: BrainFlow repo has rust_package/brainflow with version 0.0.1 in Cargo.toml; C++ releases are tagged (e.g. 5.13.0).
   - What’s unclear: Whether Rust and C++ are always in sync in a single tag.
   - Recommendation: Pin one BrainFlow git tag (e.g. 5.13.0 or later) for both C++ and Rust; document that tag in docs/brainflow.md. If the repo uses a single tag for both, use that; otherwise document “use commit X for Rust binding compatible with C++ tag Y”.

3. **Board-specific params (BRAIN-09)**
   - Deferred to future; Phase 10 scope is BRAIN-06, BRAIN-07, BRAIN-08. Hub already has board_id and serial_port in Settings; extra params (timeout, presets) can be added later.

---

## Sources

### Primary (HIGH confidence)

- [BuildBrainFlow](https://brainflow.readthedocs.io/en/stable/BuildBrainFlow.html) — C++ build (tools/build.py), Rust (build from source, C++ first); build.rs expects lib/ in crate.
- [DataFormatDesc](https://brainflow.readthedocs.io/en/stable/DataFormatDesc.html) — get_board_data shape, get_timestamp_channel, get_eeg_channels, units (µV), presets.
- [User API](https://brainflow.readthedocs.io/en/stable/UserAPI.html) — prepare_session, start_stream, get_board_data, stop_stream, release_session.
- brainflow-dev/brainflow: `tools/build.py`, `rust_package/brainflow/Cargo.toml`, `rust_package/brainflow/build.rs` (build.rs reads CARGO_MANIFEST_DIR/lib, link-search, link-lib).
- Codebase: `crates/neurohid-device/src/brainflow.rs`, `crates/neurohid-core/src/tasks/device/streaming.rs`, `crates/neurohid-types/src/signal.rs` (Sample), `.planning/research/STACK.md`.

### Secondary (MEDIUM confidence)

- Phase 9 research (09-RESEARCH.md) — synthetic adapter, DeviceProvider/Device, BRAIN-01–05; Phase 10 builds on same abstraction.
- docs/brainflow.md — current doc; Phase 10 adds native SDK and build order here.
- REQUIREMENTS.md — BRAIN-06, BRAIN-07, BRAIN-08.

### Tertiary (LOW confidence)

- BrainFlow Rust API surface (BoardShim, get_board_data return type) — assumed from docs and build layout; exact function names in the binding should be verified when implementing.

---

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — official BrainFlow build and Rust binding; only integration (feature flag, lib path, Sample mapping) is new.
- Architecture: HIGH — existing Device/SampleStream pipeline; native is another Device implementation.
- Pitfalls: HIGH — build order and default/CI feature discipline are well identified; data mapping is standard from BrainFlow docs.

**Research date:** 2026-02-21  
**Valid until:** ~30 days; re-check Rust binding API when implementing.

---

## RESEARCH COMPLETE
