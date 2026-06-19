# Residual Review Findings

This file is the durable residual sink for individual LFG findings that were too large or environment-dependent to complete safely in this branch. See `docs/plans/2026-06-19-001-fix-review-findings-plan.md` for the traceability plan.

## F1: P1 Unauthenticated Local IPC Control

**Status:** Residual security design required.

`crates/neurohid-service/src/ipc_server/mod.rs` still accepts local control RPC envelopes for mutating commands without a capability check. This branch did not add a quick token gate because the correct fix affects multiple local clients and needs an explicit trust model.

Acceptance criteria for follow-up:

- Separate read-only requests (`Snapshot`, `TrainerSnapshot`, runtime event subscriptions) from mutating requests (`Shutdown`, output/calibration/learning toggles, model promotion, stream connect/disconnect, recording commands).
- Require an unguessable per-session capability, OS credential validation, or equivalent local authorization proof for all mutating commands.
- Ensure Python, hub, CLI, and service clients obtain and send that proof through one shared helper.
- Add negative tests proving unauthenticated mutating requests are rejected and read-only requests remain usable as intended.
- Document migration behavior for existing local workflows.

## F10: P1/P2 Encrypted Training Log Append Performance

**Status:** Residual storage redesign required.

`crates/neurohid-storage/src/profile.rs` still rewrites the full encrypted `TrainingSessionLog` every time `append_training_episode` is called. This branch did not change the storage format because a safe fix requires a format/migration decision, not just a local micro-optimization.

Acceptance criteria for follow-up:

- Replace whole-log rewrite-on-append with encrypted append-friendly segments, a journal, or another format that keeps per-episode append cost bounded.
- Preserve existing encrypted session logs through a migration or dual-read path.
- Define compaction behavior for long sessions.
- Add performance coverage showing append time does not grow linearly with accumulated episode count.
- Keep exported plaintext session logs compatible with existing training tools.

## F11: BrainFlow Native All-Features Environment

**Status:** Residual CI/toolchain setup required if CI lacks native BrainFlow libraries.

Prior verification showed `cargo clippy --workspace --all-targets --all-features -- -D warnings` and all-features tests failed because the BrainFlow native dependency's `lib` path was absent. This branch must not weaken all-features checks to hide that failure.

Acceptance criteria for follow-up:

- Ensure the CI image or setup step installs/builds the BrainFlow native library before all-features Rust checks.
- Keep default Rust checks independent of BrainFlow native setup where possible.
- Document local setup in `docs/brainflow.md` or CI docs with `uv`-based commands where Python tooling is involved.
- Record any remaining all-features failure in the PR body if CI remains red.

## F11: Default Rust Missing-Docs Gate

**Status:** Residual documentation sweep required if default clippy still fails on pre-existing public API docs.

Prior verification showed default Rust clippy failed on denied missing docs in public types before tests could run. Verification in this branch still fails in `neurohid-types` with hundreds of pre-existing missing-docs diagnostics and one pre-existing `clippy::float_cmp` test assertion. This branch adds docs for newly introduced public recording manifest fields, but it does not complete a repository-wide documentation/lint sweep for pre-existing public items.

Acceptance criteria for follow-up:

- Run `cargo clippy --workspace --all-targets -- -D warnings` from a clean checkout.
- Add meaningful documentation to each public item reported by the missing-docs lint, without weakening the lint.
- Replace strict float equality assertions reported by clippy with tolerance-based assertions where appropriate.
- Keep docs focused on API behavior and invariants rather than repeating field names.
- Re-run default Rust clippy and workspace tests after the docs sweep.

## Partially Addressed Findings to Revisit

- F7 recording now records lag counters and provenance fields, but a fuller policy for plaintext output locations and retention should be designed before research deployments depend on recordings as regulated provenance.
- F9 stream and LSL failures now surface more directly, but long-running empirical validation with real LSL streams should confirm thresholds and UI messaging are appropriate for lab hardware.
