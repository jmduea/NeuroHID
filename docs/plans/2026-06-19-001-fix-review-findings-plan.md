---
title: "fix: Address review findings for research-grade runtime safety"
date: 2026-06-19
type: fix
origin: prior comprehensive repository audit
scope: lfg-pipeline
---

# fix: Address Review Findings for Research-Grade Runtime Safety

## Summary

This plan handles the prior review as eleven individually traceable findings rather than a bulk cleanup. The implementation should fix the safest coherent subset in this branch, verify each changed contract with focused tests, and make any remaining high-risk or oversized findings durable as explicit residual work in tracked documentation and the PR body.

The primary implementation target is runtime correctness and user-facing honesty for scientific lab use: timestamp domains must not be mixed, Python callers must emit the Rust control contract, UI surfaces must not advertise unvalidated readiness, recording must surface data-loss/provenance gaps, and known verification gates should become green where the repository environment allows it.

---

## Problem Frame

The reviewed repository is intended for neuroscience and BCI research workflows where "looks connected", "training complete", or "feedback success" can materially mislead a lab operator. Several findings allow local mutation without an explicit trust model, mix sample and wall-clock time domains, hide degraded stream or recording states, or present simulated/no-data paths as successful runtime operation.

This branch must prioritize corrections that reduce false confidence and strengthen protocol contracts without inventing a large new security architecture in one PR.

---

## Requirements Trace

- F1: Local IPC mutating control requests require an authentication or capability model, or a durable residual plan if the security design is too broad for this cycle.
- F2: LSL timestamps must not be converted into wall-clock `device_timestamp` values, and ErrP release/sample-window logic must use one coherent timestamp domain.
- F3: Python control helpers must serialize `ControlRequest { request_id, command }`, while preserving ergonomic bare-command inputs at the Python API boundary if useful.
- F4: Python Lab must launch the JSON-lines lab kernel command, not the Jupyter command.
- F5: Connected stream state must clear when a stream task exits unexpectedly.
- F6: Simulated or no-data ErrP feedback must not be counted as validated success.
- F7: Recording must surface lag/data loss, reject surprising plaintext/default output when appropriate, and improve manifest provenance.
- F8: Calibration UI copy must state that calibration data was collected and model validation is pending unless a validated model artifact is actually present.
- F9: Stream task false health and LSL pull errors must surface degraded states rather than silently continuing or stalling.
- F10: Whole-log encrypted rewrites for every training episode must be fixed if feasible, or recorded as residual performance work with migration and storage-design notes.
- F11: Known verification failures must be addressed where possible: Python Ruff format, Rust missing docs, and all-features BrainFlow native-library failure documentation.

---

## Key Technical Decisions

- Use `system_timestamp` for runtime ErrP window release and sample collection. LSL timestamps are arbitrary-epoch LSL clock values, so using them as `device_timestamp` creates cross-domain comparisons with decision timestamps and wall clock.
- Preserve Python API ergonomics by normalizing bare command objects into full `ControlRequest` envelopes in `neurohid_ml.control`. Rust and IPC bindings should continue to require the explicit request shape.
- Treat stream task exit as a state transition owned by the device task. Per-stream tasks can report degradation, but the parent map is authoritative for `device_connected` and discovered-stream `connected`.
- Keep IPC authentication and encrypted append-log redesign as durable residuals unless implementation reveals an existing local capability mechanism. Both affect security/storage contracts beyond a narrow patch.
- Prefer honest UI copy over fake readiness. Calibration completion should not claim a usable trained profile unless runtime/profile state proves a decoder artifact is available.

---

## Implementation Units

### U1. Normalize Timestamp Domains for ErrP and LSL

**Requirements:** F2, F9

**Dependencies:** None

**Files:**
- `crates/neurohid-device/src/lsl/device.rs`
- `crates/neurohid-core/src/tasks/ipc/mod.rs`
- `crates/neurohid-core/src/tasks/ipc/broker_task.rs`
- Rust tests in the touched modules

**Approach:** LSL samples should keep the LSL clock out of `device_timestamp` unless a later design explicitly carries timestamp-domain metadata. ErrP sample buffers, watermarks, window collection, and sample-rate estimation should use `system_timestamp` because decision timestamps and grace deadlines are runtime wall-clock microseconds.

**Patterns to follow:** Existing `Sample` construction in device backends and existing IPC task unit tests around pending ErrP windows.

**Test scenarios:**
- LSL sample construction produces `system_timestamp` from `now_micros()` and does not map an arbitrary LSL timestamp into `device_timestamp`.
- ErrP pending windows release when wall-clock sample timestamps pass `window_end_us`.
- Sample collection for an ErrP window ignores incompatible device timestamps when `system_timestamp` falls inside the window.
- Estimated ErrP sample rate is based on the same timestamp domain used for collection.

**Verification:** Focused Rust tests pass for `neurohid-device` and `neurohid-core` timestamp behavior.

### U2. Fix Python Control Request Contract

**Requirements:** F3

**Dependencies:** None

**Files:**
- `python/src/neurohid_ml/control.py`
- `python/tests/test_control_client.py`

**Approach:** Add a small request-normalization helper that accepts either a full `ControlRequest` or a bare `ControlCommand`, emits `{"request_id": ..., "command": ...}`, and ensures convenience methods use the Rust contract. Keep type hints and focused tests around serialized JSON.

**Patterns to follow:** Existing control-client tests with fake runtime capture.

**Test scenarios:**
- `set_fallback_policy()` sends a JSON string whose top-level object contains `request_id` and `command`.
- `dispatch_control()` passes through an already wrapped request without double-wrapping.
- Invalid non-dict policy still raises `NotebookError`.

**Verification:** Python control tests pass under `uv run --project python pytest python/tests/test_control_client.py`.

### U3. Launch Python Lab Kernel Adapter

**Requirements:** F4

**Dependencies:** None

**Files:**
- `apps/neuroide-hub/src/app/mod.rs`
- `apps/neuroide-hub/src/screens/python_lab.rs`
- Hub tests if an existing screen-level hook is practical

**Approach:** Stop feeding `ui.jupyter_command` to `PythonLabScreen`. Use the existing lab-kernel command (`uv run --project python neurohid-ml lab-kernel`) as the Python Lab command, while leaving Jupyter IDE behavior unchanged.

**Patterns to follow:** Existing `PythonLabScreen` test fixture that already references the kernel adapter command.

**Test scenarios:**
- Python Lab default command string references `neurohid-ml lab-kernel`, not `jupyter lab`.
- Jupyter IDE still uses `ui.jupyter_command`.

**Verification:** Hub unit tests compile for the app module or a smaller helper test validates the command selection.

### U4. Clear Stale Stream Health on Task Exit

**Requirements:** F5, F9

**Dependencies:** None

**Files:**
- `crates/neurohid-core/src/tasks/device/mod.rs`
- `crates/neurohid-core/src/tasks/device/streaming.rs`
- Rust tests in device task/streaming modules where practical

**Approach:** Detect finished per-stream join handles during the interactive device loop and remove them from `active_streams`. Mark the discovered stream disconnected/degraded, update aggregate `device_connected`, and register a device integrity issue when a stream exits without an explicit disconnect.

**Patterns to follow:** Existing `update_connection_state`, `set_stream_connected`, and `report_device_integrity_issue` state mutation helpers.

**Test scenarios:**
- A completed stream task is pruned from the active map and aggregate state reports disconnected when it was the last stream.
- Explicit disconnect still clears connected state without double-reporting unexpected degradation.
- Stream errors or EOF produce a visible degraded/integrity state.

**Verification:** Focused Rust tests cover helper behavior, and clippy stays clean.

### U5. Stop Treating Simulated ErrP as Valid Success

**Requirements:** F6

**Dependencies:** U1

**Files:**
- `crates/neurohid-core/src/tasks/ipc/mod.rs`
- `crates/neurohid-types/src/config.rs`
- Runtime/IPC tests

**Approach:** In simulation mode, avoid emitting neutral "good" ErrP results that downstream metrics can treat as successful feedback. Mark bridge mode as simulated/fallback/degraded and either withhold ErrP results or emit an explicit unusable/no-data signal that cannot count as validated success.

**Patterns to follow:** Existing `RuntimeModeState`, `SignalQuality`, and runtime capability degradation messaging.

**Test scenarios:**
- Simulation mode does not send an ErrP result with `SignalQuality::Good` and zero latency/confidence.
- Runtime snapshot shows simulated bridge state and limited capability messaging.
- Real trainer ErrP results continue to pass through after validation.

**Verification:** IPC task unit tests or runtime handle tests cover simulated behavior.

### U6. Improve Recording Loss Visibility and Provenance

**Requirements:** F7

**Dependencies:** None

**Files:**
- `crates/neurohid-core/src/tasks/recording.rs`
- `crates/neurohid-types/src/recording.rs`
- Recording tests if existing harness permits

**Approach:** Promote sample/action broadcast lag from debug/no-op to warn and count it in session metadata. Add runtime version, SDK version if known, and device stream summary at stop time. Validate output roots enough to avoid accidental empty/default plaintext paths being presented as research-grade provenance.

**Patterns to follow:** Existing `SessionManifest` lifecycle update on stop and state snapshots in `ServiceState`.

**Test scenarios:**
- Lagged sample and action receivers increment durable counters.
- Manifest written at stop includes end time and loss/provenance fields.
- Start rejects empty output overrides and resolves the configured default predictably.

**Verification:** Recording tests cover manifest serialization and lag counter behavior.

### U7. Make Calibration Readiness Copy Honest

**Requirements:** F8

**Dependencies:** None

**Files:**
- `apps/neuroide-hub/src/calibration/panel.rs`
- Existing calibration panel tests

**Approach:** Replace "Training completed" and "profile calibrated and ready" claims with copy that says calibration data collection is complete, candidate training/validation must be confirmed, and normal operation resumes with current validated model state.

**Patterns to follow:** Existing calibration panel state tests and UI string assertions if available.

**Test scenarios:**
- Completion screen copy no longer claims the profile is ready to use.
- Decoder-training step copy no longer implies a validated model artifact exists.

**Verification:** Calibration panel tests pass and compile.

### U8. Fix Verification Gate Formatting and Docs

**Requirements:** F11

**Dependencies:** Depends on files touched by implementation

**Files:**
- `python/src/neurohid_ml/cli.py`
- `python/tests/test_control_client.py`
- `python/tests/test_lab_kernel.py`
- `python/tests/test_notebook_helpers.py`
- Public Rust types flagged by clippy

**Approach:** Run Ruff format on Python files and add missing Rust docs for public items surfaced by default clippy. Do not weaken lints or assertions.

**Patterns to follow:** Workspace AGENTS.md and existing Rust module docs.

**Test scenarios:** Formatting/lint checks are the test.

**Verification:** `uv run --project python ruff format --check python/src python/tests`, `uv run --project python ruff check python/src python/tests`, and default Rust clippy progress beyond missing-doc failures introduced by this branch.

### U9. Record IPC Authentication Residual

**Requirements:** F1

**Dependencies:** None

**Files:**
- `docs/residual-review-findings/jmduea-lfg-review-findings.md`
- PR body

**Approach:** If no existing capability/token mechanism is present, record a durable residual specifying the required design: separate read-only events from mutating control, add per-session local capability or OS-credential checks, reject mutating commands without proof, and include migration/testing requirements.

**Test scenarios:** Residual contains affected commands and acceptance criteria.

**Verification:** Residual is committed and linked from the PR.

### U10. Record Encrypted Append-Log Residual

**Requirements:** F10

**Dependencies:** None

**Files:**
- `docs/residual-review-findings/jmduea-lfg-review-findings.md`
- PR body

**Approach:** If the storage design is too risky for this LFG cycle, record a durable residual describing append-friendly encrypted segments or journal files, compaction strategy, migration path, and performance tests.

**Test scenarios:** Residual includes measurable performance acceptance criteria.

**Verification:** Residual is committed and linked from the PR.

### U11. Record BrainFlow Native All-Features Residual

**Requirements:** F11

**Dependencies:** None

**Files:**
- `docs/residual-review-findings/jmduea-lfg-review-findings.md`
- PR body

**Approach:** If CI lacks BrainFlow native libs, record this as an environment/toolchain residual rather than weakening all-features builds. Include the failing feature path and expected setup path.

**Test scenarios:** Residual names the all-features build failure and points to native library setup.

**Verification:** PR body documents unresolved all-features status if CI remains red.

---

## Scope Boundaries

### In Scope

- Focused code fixes for timestamp domains, Python control contracts, Python Lab command selection, stale stream health, simulated feedback honesty, recording provenance/loss visibility, calibration copy, and verification-format issues.
- Durable residual documentation for large security/storage/environment findings that cannot be safely completed in this branch.

### Deferred to Follow-Up Work

- Full IPC authentication/capability architecture if no narrow local trust mechanism already exists.
- Append-friendly encrypted training log storage if the current storage abstraction cannot support safe incremental writes without migration design.
- CI image/toolchain changes for BrainFlow native all-features builds if the dependency is missing from the runner.

### Out of Scope

- Replacing the IPC transport stack.
- Rewriting model training or calibration algorithms.
- Adding new browser UI automation infrastructure beyond compile/unit verification for egui surfaces.

---

## Risks and Mitigations

- Timestamp changes can affect downstream training windows. Mitigate with focused tests that prove collection, release, and sample-rate estimation use one domain.
- Stream health pruning can race with explicit disconnect. Mitigate by distinguishing explicit cancellation from unexpected task completion.
- Recording manifest changes affect serialized format. Mitigate with serde defaults where new fields are added.
- IPC auth is security-sensitive. Mitigate by documenting residual acceptance criteria rather than shipping a token model without design review.

---

## Verification Plan

- Python: `uv run --project python ruff format --check python/src python/tests`; `uv run --project python ruff check python/src python/tests`; `uv run --project python pytest python/tests`.
- Rust default: `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`.
- Rust all-features: `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features`, with BrainFlow native failures recorded as residual if the environment cannot link the native library.
- UI/browser: no browser path is expected because this is an egui native desktop application; use unit/compile tests and document why browser testing is not applicable.

---

## Acceptance Criteria

- Every individual finding F1-F11 is either fixed in code or appears in durable residual documentation and the PR body.
- Python control requests match Rust `ControlRequest`.
- ErrP timing no longer mixes arbitrary LSL timestamps with wall-clock decision timestamps.
- UI surfaces no longer overstate Python Lab or calibration readiness.
- Recording lag/provenance issues are visible in logs/manifests.
- Verification outcomes and any environmental failures are documented in the PR.
