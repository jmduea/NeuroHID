# Per-Finding LFG Evidence

This document closes the traceability gap for PR #13 by recording how each individual audit finding F1-F11 moved through the LFG phases. The implementation used one aggregate plan, `docs/plans/2026-06-19-001-fix-review-findings-plan.md`, but that plan enumerated every finding and assigned each finding to one or more implementation or residual units.

## Shared LFG Evidence

- Plan phase: `docs/plans/2026-06-19-001-fix-review-findings-plan.md` lines 27-39 enumerate F1-F11 as individual requirements, and lines 53-274 assign them to units U1-U11.
- Implementation phase: commit `700e723` (`fix: harden runtime review findings`) contains the code/documentation changes for the implemented and residual units.
- Review phase: `ce-code-review` was run in agent mode against the aggregate plan before the implementation commit was finalized. Eligible fixes were applied and persisted in commit `700e723`; remaining oversized or environmental items were handed off in `docs/residual-review-findings/jmduea-lfg-review-findings.md`.
- PR evidence: PR #13 (`https://github.com/jmduea/NeuroHID/pull/13`) links the plan, residual file, ADR reference, verification outcomes, and this per-finding evidence file.
- Browser/UI phase: no browser test path applies because NeuroIDE Hub is an egui native desktop app; UI coverage is by Rust compile/unit tests and documented in the PR body.
- CI phase: required PR checks passed after the ADR body update and CI refresh commit `fa18469` (`chore: refresh ci after adr reference`).

## F1: P1 Unauthenticated Local IPC Control

- Plan requirement: F1 in the requirements trace; implementation unit U9.
- Outcome: residual, not fixed in code.
- Reason: a safe fix needs a local trust/capability architecture spanning service, hub, CLI, and Python clients.
- Review result: reviewed as part of the aggregate plan; kept as a residual rather than accepting an under-designed token gate.
- Residual handoff: `docs/residual-review-findings/jmduea-lfg-review-findings.md`, section `F1: P1 Unauthenticated Local IPC Control`.
- Verification evidence: residual acceptance criteria require negative tests for unauthenticated mutating commands and preservation of read-only requests.
- Commit/PR evidence: residual file added in commit `700e723`; PR #13 residual section names `crates/neurohid-service/src/ipc_server/mod.rs`.

## F2: P1 LSL Timestamp-Domain Mixing

- Plan requirement: F2 in the requirements trace; implementation unit U1.
- Outcome: fixed.
- Implementation evidence: `crates/neurohid-device/src/lsl/device.rs` no longer maps arbitrary-epoch LSL timestamps into `Sample.device_timestamp`; `crates/neurohid-core/src/tasks/ipc/mod.rs` uses the runtime `system_timestamp` domain for ErrP window logic.
- Review result: reviewed as part of U1; no residual remains for timestamp-domain mixing.
- Residual handoff: none.
- Verification evidence: `cargo test -p neurohid-core sample_runtime_timestamp_uses_system_clock_domain` passed; `cargo test --workspace` passed.
- Commit/PR evidence: files modified in commit `700e723`; PR #13 lists F2 as fixed.

## F3: P1 Python Control Request Contract Mismatch

- Plan requirement: F3 in the requirements trace; implementation unit U2.
- Outcome: fixed.
- Implementation evidence: `python/src/neurohid_ml/control.py` normalizes bare commands and wrapped requests into `ControlRequest { request_id, command }`; `python/tests/test_control_client.py` verifies serialized request shape.
- Review result: reviewed as part of U2; no residual remains for the Python control contract.
- Residual handoff: none.
- Verification evidence: `uv run --project python --with pytest pytest python/tests` passed with 39 passed and 1 skipped; Ruff format/check passed.
- Commit/PR evidence: files modified in commit `700e723`; PR #13 lists F3 as fixed.

## F4: P1 Python Lab Uses Wrong Command

- Plan requirement: F4 in the requirements trace; implementation unit U3.
- Outcome: fixed.
- Implementation evidence: `apps/neuroide-hub/src/screens/python_lab.rs` defines the JSON-lines lab kernel command, and `apps/neuroide-hub/src/app/mod.rs` passes it to `PythonLabScreen` instead of the Jupyter command.
- Review result: reviewed as part of U3; no residual remains for Python Lab command selection.
- Residual handoff: none.
- Verification evidence: `cargo test --workspace` passed; PR browser/UI section documents why browser automation is not applicable.
- Commit/PR evidence: files modified in commit `700e723`; PR #13 lists F4 as fixed.

## F5: P1 Stale Connected Stream State

- Plan requirement: F5 in the requirements trace; implementation unit U4.
- Outcome: fixed.
- Implementation evidence: `crates/neurohid-core/src/tasks/device/mod.rs` prunes finished stream tasks and updates connection state; `crates/neurohid-core/src/tasks/device/streaming.rs` records `StreamTaskExited` integrity issues.
- Review result: reviewed as part of U4; remaining real-hardware validation is tracked under F9.
- Residual handoff: none for stale connected state itself.
- Verification evidence: `cargo test --workspace` passed.
- Commit/PR evidence: files modified in commit `700e723`; PR #13 lists F5 as fixed.

## F6: P1 Simulated/Uncalibrated ErrP Feedback Treated as Success

- Plan requirement: F6 in the requirements trace; implementation unit U5.
- Outcome: fixed.
- Implementation evidence: `crates/neurohid-core/src/tasks/ipc/mod.rs` suppresses simulated neutral ErrP feedback and marks simulated trainer bridge capability as degraded.
- Review result: reviewed as part of U5; no residual remains for simulated feedback being counted as validated success.
- Residual handoff: none.
- Verification evidence: `cargo test --workspace` passed, including the IPC/runtime code touched by the change.
- Commit/PR evidence: file modified in commit `700e723`; PR #13 lists F6 as fixed.

## F7: P2 Recording Drops/Lag/Provenance Gaps

- Plan requirement: F7 in the requirements trace; implementation unit U6.
- Outcome: partially fixed with residual follow-up.
- Implementation evidence: `crates/neurohid-core/src/tasks/recording.rs` tracks sample/action lag, validates empty output overrides, and writes runtime/device provenance; `crates/neurohid-types/src/recording.rs` adds durable lag counters to `SessionManifest`.
- Review result: reviewed as part of U6; fuller plaintext-output policy and retention/provenance design remained residual.
- Residual handoff: `docs/residual-review-findings/jmduea-lfg-review-findings.md`, section `Partially Addressed Findings to Revisit`.
- Verification evidence: `cargo test --workspace` passed; Rust clippy residuals are separately documented under F11.
- Commit/PR evidence: files modified in commit `700e723`; PR #13 lists F7 as partially fixed with residual follow-up.

## F8: P2 Calibration UI Overstates Readiness

- Plan requirement: F8 in the requirements trace; implementation unit U7.
- Outcome: fixed.
- Implementation evidence: `apps/neuroide-hub/src/calibration/panel.rs` now says calibration data was collected and validated model readiness is pending instead of claiming the profile is ready.
- Review result: reviewed as part of U7; no residual remains for misleading readiness copy.
- Residual handoff: none.
- Verification evidence: `cargo test --workspace` passed; PR browser/UI section documents egui native UI verification path.
- Commit/PR evidence: file modified in commit `700e723`; PR #13 lists F8 as fixed.

## F9: P1 Stream/Task False Health and LSL Pull Reliability

- Plan requirement: F9 in the requirements trace; implementation units U1 and U4.
- Outcome: partially fixed with residual follow-up.
- Implementation evidence: `crates/neurohid-device/src/lsl/device.rs` surfaces sustained LSL pull failures as a communication error; device task pruning in `crates/neurohid-core/src/tasks/device/mod.rs` prevents finished stream tasks from staying connected.
- Review result: reviewed as part of U1 and U4; real LSL hardware threshold and UI validation remain residual.
- Residual handoff: `docs/residual-review-findings/jmduea-lfg-review-findings.md`, section `Partially Addressed Findings to Revisit`.
- Verification evidence: `cargo test --workspace` passed; focused timestamp-domain test passed.
- Commit/PR evidence: files modified in commit `700e723`; PR #13 lists F9 as partially fixed with residual follow-up.

## F10: P1/P2 Encrypted Training Log Append Performance

- Plan requirement: F10 in the requirements trace; implementation unit U10.
- Outcome: residual, not fixed in code.
- Reason: changing `crates/neurohid-storage/src/profile.rs` safely requires a storage-format and migration design for encrypted logs.
- Review result: reviewed as part of U10; kept as a durable storage redesign residual rather than shipping an unsafe format change.
- Residual handoff: `docs/residual-review-findings/jmduea-lfg-review-findings.md`, section `F10: P1/P2 Encrypted Training Log Append Performance`.
- Verification evidence: residual acceptance criteria require bounded append performance coverage, migration/dual-read support, and compaction behavior.
- Commit/PR evidence: residual file added in commit `700e723`; PR #13 residual section names `crates/neurohid-storage/src/profile.rs`.

## F11: P2/P3 Verification Gate Issues

- Plan requirement: F11 in the requirements trace; implementation units U8 and U11.
- Outcome: partially fixed with residual follow-up.
- Implementation evidence: Ruff formatting was applied to `python/src/neurohid_ml/cli.py`, `python/tests/test_control_client.py`, `python/tests/test_lab_kernel.py`, and `python/tests/test_notebook_helpers.py`.
- Review result: reviewed as part of U8 and U11; pre-existing Rust missing-docs, `clippy::float_cmp`, and BrainFlow native all-features setup remained residual.
- Residual handoff: `docs/residual-review-findings/jmduea-lfg-review-findings.md`, sections `F11: BrainFlow Native All-Features Environment` and `F11: Default Rust Missing-Docs Gate`.
- Verification evidence: `uv run --project python ruff format --check python/src python/tests` passed; `uv run --project python ruff check python/src python/tests` passed; `cargo clippy --workspace --all-targets -- -D warnings` and all-features clippy failures are documented residuals.
- Commit/PR evidence: Python files and residual file modified in commit `700e723`; PR #13 lists the Python formatting portion of F11 as fixed and Rust/BrainFlow portions as residual.
