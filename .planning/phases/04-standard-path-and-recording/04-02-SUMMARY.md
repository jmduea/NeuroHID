---
phase: 04-standard-path-and-recording
plan: 02
subsystem: recording
tags: [session, manifest, jsonl, control, neurohid-core, neurohid-types]

# Dependency graph
requires:
  - phase: 04-standard-path-and-recording
    provides: CONTEXT and RESEARCH (session folder layout, tap pattern)
provides:
  - Session folder layout and recording types (SessionManifest, RecordingConfig)
  - Recording tap task writing manifest, config snapshot, profile_meta, streams/, actions.jsonl
  - Control commands StartRecording/StopRecording and responses RecordingStarted/RecordingStopped
  - Snapshot fields recording_active, current_session_id
  - CLI record start/stop/status subcommands
affects: 04-03 (export/replay will consume session folder)

# Tech tracking
tech-stack:
  added: []
  patterns: recording tap (broadcast subscribe + command channel + oneshot reply), session folder layout

key-files:
  created: crates/neurohid-types/src/recording.rs, crates/neurohid-core/src/tasks/recording.rs
  modified: crates/neurohid-types/src/config.rs, lib.rs, control.rs, ipc.rs, crates/neurohid-core/src/service.rs, runtime.rs, tasks/mod.rs, crates/neurohid-hub/src/service_manager.rs, crates/neurohid/src/bin/neurohid-service.rs, neurohid-validate.rs, docs/formats/config-format.md

key-decisions:
  - "Recording config default_output_path as Option<String> for config file compatibility"
  - "Config snapshot in session folder as config.json (serde_json) for minimal deps in neurohid-core"
  - "dispatch_control_request made async to await recording task oneshot reply"

patterns-established:
  - "Recording tap: separate task with sample_broadcast_tx.subscribe(), action_broadcast_tx.subscribe(), command channel (RecordingCommand + oneshot::Sender<Result<RecordingCommandResult>>)"

requirements-completed: [PATH-02]

# Metrics
duration: 0
completed: 2026-02-20
---

# Phase 4 Plan 2: Session Recording Summary

**Session recording pipeline: session folder (manifest, config snapshot, profile_meta, streams/, actions.jsonl), recording tap task in neurohid-core, control Start/StopRecording and CLI record start/stop/status.**

## Performance

- **Duration:** (single session)
- **Tasks:** 3
- **Files created:** 2 (recording.rs in types and core/tasks)
- **Files modified:** 12+

## Accomplishments

- Session manifest and recording config types; SystemConfig.recording with defaults
- Recording tap task: subscribes to sample/action broadcast, writes session folder on Start, flushes and sets ended_at on Stop
- Control protocol: StartRecording { output_path }, StopRecording; RecordingStarted/Stopped responses; snapshot recording_active, current_session_id
- Service spawns recording task with command channel; runtime dispatch_control_request async, handles recording commands via oneshot
- CLI: neurohid-service record start [--output-path], stop, status
- config-format.md documents recording config and per-session path override

## Task Commits

1. **Task 1: Session folder layout and recording types** - `dbb0654` (feat)
2. **Task 2: Recording tap task and service integration** - `4907a5f` (feat)
3. **Task 3: CLI and config surface** - `cd1567f` (feat)

## Files Created/Modified

- `crates/neurohid-types/src/recording.rs` - SessionManifest, RecordingConfig, RecordingAutoMode
- `crates/neurohid-core/src/tasks/recording.rs` - RecordingTask, session folder creation and write loop
- `crates/neurohid-types/src/config.rs` - SystemConfig.recording
- `crates/neurohid-types/src/control.rs` - StartRecording/StopRecording, RecordingStarted/Stopped, snapshot fields
- `crates/neurohid-types/src/ipc.rs` - ControlRpcResponsePayload recording variants
- `crates/neurohid-core/src/service.rs` - recording state fields, recording task spawn, recording_command_tx on handle
- `crates/neurohid-core/src/runtime.rs` - snapshot recording fields, async dispatch_control_request, recording command handling
- `crates/neurohid/src/bin/neurohid-service.rs` - record subcommand (start/stop/status), run_record_command
- `docs/formats/config-format.md` - recording config section, CLI override note

## Decisions Made

- Recording default path as Option<String> in config for YAML/TOML portability
- Session config snapshot as config.json (JSON) to avoid adding serde_yaml to neurohid-core
- Control handler made async and span dropped before await to satisfy Send for tokio::spawn

## Deviations from Plan

None - plan executed as written. Minor cleanups: unused variable renames in recording task; IPC/hub/validate match arms for new payload variants; Send fix for control handler (drop span before await).

## Issues Encountered

- handle_control_request_envelope held EnteredSpan across .await, breaking Send; fixed by dropping span before await
- Multiple crates had exhaustive matches on ControlResponsePayload/ControlCommand; added RecordingStarted/Stopped and StartRecording/StopRecording arms

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Session folder and recording pipeline ready for Plan 03 (export to XDF, replay)
- Auto_mode (TiedToRuntime / TiedToOutput) not implemented; can be added when runtime start/output toggle are wired to send Start/StopRecording

## Self-Check

- SUMMARY.md created: FOUND
- Task commits present: dbb0654, 4907a5f, cd1567f (verified via git log)
- Key files exist: recording.rs (types + core/tasks), config/control/service/runtime/CLI updated

**Self-Check: PASSED**

---
*Phase: 04-standard-path-and-recording*
*Plan: 02*
*Completed: 2026-02-20*
