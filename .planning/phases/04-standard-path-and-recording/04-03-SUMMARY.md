# Plan 04-03: Export to XDF and Replay — Summary

**Completed:** 2026-02-20

## Key files created

- `crates/neurohid-core/src/recording/xdf_writer.rs` — XDF 1.0 writer from session folder (FileHeader, StreamHeader, Samples, StreamFooter; tags 1,2,3,6 for pyxdf/xdf_rs compatibility).
- `crates/neurohid-core/src/recording/mod.rs` — Recording module; re-exports `export_session_to_xdf()`.
- `crates/neurohid-core/src/tasks/replay_source.rs` — File-backed sample source: `load_session_samples()`, `run_replay_task()` from session folder streams.

## Key files modified

- `crates/neurohid-core/src/lib.rs` — Added `pub mod recording`.
- `crates/neurohid-core/src/tasks/mod.rs` — Added `replay_source`, re-export `load_session_samples`, `run_replay_task`.
- `crates/neurohid-core/src/service.rs` — Optional `replay_session_path`, `with_replay_path()`; spawn replay task instead of device when set; device handle result type unified to `()`.
- `crates/neurohid-core/src/runtime.rs` — `RuntimeBuilder::with_replay_path()`, pass through to service.
- `crates/neurohid/src/bin/neurohid-service.rs` — `record export <session_dir> -o <out.xdf>`; `record replay-offline <session_dir>`; `--replay <path>`; `load_runtime_context(..., replay_path)`; `RuntimeContext.replay_path`.
- `crates/neurohid-core/Cargo.toml` — Dev-dependency `xdf = "0.1.2"` for round-trip test.
- `docs/user-guide.md` — New subsection "Recording and export" (session folders, export to XDF, EEGLAB/MNE/pyxdf, replay and replay-offline).

## What was built

- **XDF export:** Session folder (manifest, config, streams/*.jsonl) → single .xdf file; chunk length includes tag per spec; readable by pyxdf and Rust `xdf` crate.
- **Export CLI:** `neurohid-service record export <session_dir> -o <out.xdf>` (offline).
- **Replay source:** Load and merge streams from session folder, send `Sample` to the same pipeline channel the device task uses; optional `--replay <path>` or `record replay-offline <session_dir>` to run pipeline on recorded data.
- **Docs:** User-guide documents where session folders live, export command, opening .xdf in EEGLAB/MNE/pyxdf, and replay/replay-offline.

## Deviations

- XDF tag numbering uses the pyxdf/xdf_rs scheme (1=FileHeader, 2=StreamHeader, 3=Samples, 6=StreamFooter) instead of the SCCN spec (6=FileHeader, 5=StreamHeader, 4=Samples, 1=StreamFooter) so that exported files are readable by pyxdf and the `xdf` crate without extra adapters.
- Replay input is session folder only (streams/*.jsonl); XDF read path for replay was not implemented (documented as export target; replay from XDF could be added later via `xdf` crate).
- `pyxdf` was added to the Python project dependencies for verification; plan did not require it in-tree.

## Verification

- `cargo check -p neurohid-core` and `cargo check -p neurohid` pass.
- Export produces .xdf readable by `uv run --directory python python -c "import pyxdf; ..."` and by `xdf::XDFFile::from_bytes()` in a unit test.
- Replay source and `record replay-offline` run the pipeline on a session folder; `--replay` integrates with managed runtime.
