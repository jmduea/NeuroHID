---
phase: 10-brainflow-deeper
verified: 2026-02-21T00:00:00Z
status: passed
score: 5/5 must-haves verified
---

# Phase 10: BrainFlow Deeper Verification Report

**Phase Goal:** Developer can build and use the real BrainFlow SDK and streaming path; builds are reproducible.

**Verified:** 2026-02-21

**Status:** passed

**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth | Status     | Evidence |
| --- | ----- | ---------- | -------- |
| 1   | Developer can reproduce a BrainFlow native build using a pinned version and documented steps | ✓ VERIFIED | docs/brainflow.md pins tag 5.13.0; build order (C++ → Rust → neurohid-device) and optional script documented |
| 2   | Build order (C++ then Rust, then neurohid-device) is unambiguous and reproducible | ✓ VERIFIED | docs/brainflow.md "Native SDK (Phase 10)" lists steps: tools/build.py, copy to rust_package/brainflow/lib, cargo build -p neurohid-device --features brainflow,brainflow-native |
| 3   | Developer can build neurohid-device with brainflow-native and use the real BrainFlow SDK for real hardware | ✓ VERIFIED | brainflow-native feature in Cargo.toml; brainflow_native.rs implements Device + SampleStream (get_board_data → Sample); brainflow.rs connect() returns native device when brainflow-native and (board_id != 0 or serial_port set) |
| 4   | Default and CI use synthetic only (brainflow-native not enabled); no mock backend | ✓ VERIFIED | neurohid-device default = ["lsl"]; neurohid-core default = ["device-lsl", "brainflow"] (no brainflow-native). No brainflow-native in .github/workflows |
| 5   | BrainFlow (synthetic or native) streams into the same signal pipeline as LSL (Device → SampleStream → sample_tx) | ✓ VERIFIED | neurohid-core streaming.rs: device.start_streaming().await → sample_stream.next() → sample_tx.send(sample). BrainFlowNativeDevice::start_streaming returns Box::pin(NativeSampleStream) yielding Result<Sample>; same path as LSL device |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | --------- | ------ | ------- |
| docs/brainflow.md | Pinned version and full build order (C++ → Rust → neurohid-device); mentions brainflow-native | ✓ VERIFIED | Contains "5.13.0", "Native SDK (Phase 10)", tools/build.py, brainflow-native, optional script reference |
| scripts/build-brainflow-native.sh | Optional script: C++ build and lib copy | ✓ VERIFIED | Uses uv for Python, BRAINFLOW_VERSION 5.13.0, runs tools/build.py, copies installed/lib to target; doc references it |
| crates/neurohid-device/Cargo.toml | brainflow-native feature; optional BrainFlow dep; default does not include brainflow-native | ✓ VERIFIED | default = ["lsl"]; brainflow-native = ["dep:brainflow", "dep:num", "dep:tokio-util"]; brainflow git tag 5.13.0 optional |
| crates/neurohid-device/src/brainflow.rs | Native path when brainflow-native enabled; synthetic unchanged when only brainflow | ✓ VERIFIED | #[cfg(feature = "brainflow-native")] branch in connect() calls brainflow_native::connect_native for real boards; otherwise synthetic BrainFlowDevice |
| crates/neurohid-device/src/brainflow_native.rs | Native Device and SampleStream; get_board_data → Sample | ✓ VERIFIED | BrainFlowNativeDevice implements Device; start_streaming returns SampleStream (NativeSampleStream); board_data_column_to_sample maps to Sample |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| docs/brainflow.md | Exact version (git tag/commit) | Pinned version subsection | ✓ WIRED | "BrainFlow git tag `5.13.0`" and build order with build.py, brainflow-native |
| crates/neurohid-device/src/brainflow.rs, brainflow_native.rs | Device start_streaming() returns SampleStream yielding Sample | Native implementation maps get_board_data to Sample; same pipeline as streaming.rs | ✓ WIRED | start_streaming returns Ok(Box::pin(NativeSampleStream(rx))); Stream::Item = Result<Sample>; neurohid-core streaming.rs consumes stream and sends to sample_tx |
| neurohid-core/Cargo.toml | default features | Do not add brainflow-native | ✓ WIRED | default = ["device-lsl", "brainflow"]; no brainflow-native |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| BRAIN-06 | 10-02 | Developer can build and use the real BrainFlow SDK behind brainflow-native; default and CI use synthetic only | ✓ SATISFIED | brainflow-native feature and optional brainflow dep; default/CI do not enable it; docs state default and CI synthetic-only |
| BRAIN-07 | 10-02 | BrainFlow streaming path same pipeline as LSL (Device → SampleStream → pipeline) | ✓ SATISFIED | Device trait + start_streaming → SampleStream; neurohid-core spawn_stream_task uses device.start_streaming() and sample_tx.send(sample) for any Device |
| BRAIN-08 | 10-01 | BrainFlow version and build steps pinned and documented for reproducible builds | ✓ SATISFIED | docs/brainflow.md: tag 5.13.0, C++ (tools/build.py), Rust copy step, neurohid-device build command; scripts/build-brainflow-native.sh optional |

No orphaned requirements: BRAIN-06, BRAIN-07, BRAIN-08 are all claimed by 10-01/10-02 and verified above.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| (none) | — | — | — | No TODO/FIXME/placeholder in neurohid-device key files |

### Human Verification Required

Optional (goal already achieved in code and docs):

1. **Real hardware run** — With BrainFlow C++ and Rust built per docs, build `neurohid-device` with `brainflow,brainflow-native`, connect to a real board (board_id != 0 or serial_port set), start streaming. Expected: samples flow into pipeline. Why human: requires physical hardware and environment.

### Gaps Summary

None. All must-haves verified; requirements BRAIN-06, BRAIN-07, BRAIN-08 satisfied with implementation and documentation evidence.

---

_Verified: 2026-02-21_
_Verifier: Claude (gsd-verifier)_
