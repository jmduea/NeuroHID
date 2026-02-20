---
status: passed
---

# Phase 4 Verification

## Goal

User has one coherent path from device to actions and can record/replay sessions for reproducibility.

## Requirement coverage

- **PATH-01:** verified — `docs/user-guide.md` provides a single walkthrough (device → pick/connect → decoder config+profile → run) with optional branches; linked from `docs/index.md` under "Canonical Entry Points" as "User guide: standard path and workflows."
- **PATH-02:** verified — Session recording (start/stop, session folder with manifest, config snapshot, streams, actions.jsonl), export to XDF via CLI, replay (virtual source via `--replay`, offline via `record replay-offline`), and user-guide section on recording/export and opening XDF in EEGLAB/MNE/pyxdf are all present and traceable to 04-02 and 04-03 deliverables.

## must_haves check

### Plan 04-01 (Standard path doc)

| must_have | Status |
|-----------|--------|
| User can find one documented path from device to decoder to actions | ✓ `docs/user-guide.md` section "Standard path: from device to actions" |
| Path lives in a user-facing doc linked from the docs index | ✓ `docs/index.md` links "User guide: standard path and workflows" → `user-guide.md` |
| Path is informal with optional branches and minimal assumptions | ✓ Walkthrough + "Optional branches" and LSL/advanced asides |
| Artifact: `docs/user-guide.md` with "Standard path" | ✓ |
| Artifact: `docs/index.md` contains "user-guide" | ✓ |
| Key link: `docs/index.md` → `docs/user-guide.md` via Markdown link | ✓ "User guide" link in Canonical Entry Points |

### Plan 04-02 (Recording types and task)

| must_have | Status |
|-----------|--------|
| User can start and stop session recording (explicit or auto) | ✓ `StartRecording`/`StopRecording` in control; CLI `record start`/`stop`; snapshot `recording_active`, `current_session_id`; config `auto_mode` |
| Recorded session contains raw streams, config/profile snapshot, decoder output | ✓ Session folder: manifest.json, config snapshot, streams/, actions.jsonl; profile_meta when profile active (per types) |
| On failure recording stops, keeps what was written, surfaces error | ✓ Recording task and control response handle errors; state cleared on stop |
| Artifact: `crates/neurohid-types/src/recording.rs` (SessionManifest, RecordingConfig) | ✓ |
| Artifact: `crates/neurohid-core/src/tasks/recording.rs` (tap task, sample_broadcast, action_broadcast, session) | ✓ |
| Artifact: `crates/neurohid-types/src/control.rs` (StartRecording, StopRecording) | ✓ |
| Service spawns recording task with sample/action broadcast; recording writes manifest, actions.jsonl | ✓ `service.rs` spawns `RecordingTask` with broadcast subscribers; task writes session folder |

### Plan 04-03 (XDF writer, export CLI, replay, user-guide recording)

| must_have | Status |
|-----------|--------|
| User can export a session folder to XDF (primary format) | ✓ `neurohid-core/src/recording/xdf_writer.rs`; `export_session_to_xdf`; CLI `record export <session_dir> -o out.xdf` |
| User can replay a session (virtual source or offline decoder run) | ✓ `replay_source.rs` + `run_replay_task`; service `--replay <path>`; CLI `record replay-offline <session_dir>` |
| Exported files and layout documented for common tools | ✓ User-guide "Recording and export" documents XDF export and EEGLAB/MNE-Python/pyxdf |
| Artifact: `crates/neurohid-core/src/recording/xdf_writer.rs` (FileHeader, StreamHeader, write) | ✓ |
| Artifact: `crates/neurohid-core/src/tasks/replay_source.rs` (session, streams, Sample) | ✓ |
| Session folder → XDF via export command | ✓ `record export` calls `export_session_to_xdf` |
| Session/XDF → pipeline via replay source or offline run | ✓ `with_replay_path`; `run_replay_offline` |

## human_verification (if any)

Optional (not required for status):

- Run through the user-guide path once (device list → connect → config/profile → run) with a real or mock device to confirm flow.
- Export a session folder to XDF and open in pyxdf or MNE to confirm compatibility.

## gaps (if status is gaps_found)

None. Status is **passed**: all plan must_haves are satisfied and PATH-01/PATH-02 are traceable to completed work in `REQUIREMENTS.md` (Phase 4, Complete).
