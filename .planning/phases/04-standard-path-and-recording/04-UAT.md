---
status: complete
phase: 04-standard-path-and-recording
source: 04-01-SUMMARY.md, 04-02-SUMMARY.md, 04-03-SUMMARY.md
started: "2026-02-20T00:00:00Z"
updated: "2026-02-20T00:00:00Z"
---

## Current Test

## Current Test

[testing complete]

## Tests

### 1. Standard path in docs
expected: Docs index links to User guide; user guide has "Standard path: from device to actions" with device→connect→decoder→run walkthrough, informal tone, optional branches.
result: pass

### 2. Start recording via CLI
expected: With the service running, running `neurohid record start` (or `record start --output-path <path>`) succeeds and prints session_id and output path.
result: issue
reported: "Error: start recording failed: Configuration error: Missing required configuration: recording output path (dispatch works; service requires output path in config or --output-path)"
severity: major

### 3. Stop recording via CLI
expected: With a recording in progress, running `neurohid record stop` succeeds and prints session_id.
result: pass

### 4. Recording status
expected: `neurohid record status` (or control snapshot) shows whether recording is active and the current session id when applicable.
result: pass

### 5. Session folder contents
expected: After stopping a recording, the session output directory contains a session folder with manifest.json, a config snapshot, stream data (e.g. streams/ or equivalent), and actions.jsonl.
result: pass

### 6. Export session to XDF
expected: Running `neurohid record export <session_dir> -o out.xdf` produces a .xdf file that is valid (e.g. readable by pyxdf or as documented).
result: pass

### 7. Replay offline
expected: Running `neurohid record replay-offline <session_dir>` runs the pipeline on the recorded session, or the user guide clearly documents how to do so.
result: pass

### 8. User guide recording section
expected: The user guide has a "Recording and export" subsection that describes where session folders live, how to export to XDF, and how to open .xdf in EEGLAB/MNE/pyxdf (and replay/replay-offline if applicable).
result: pass

## Summary

total: 8
passed: 7
issues: 1
pending: 0
skipped: 0

## Gaps

- truth: "With the service running, neurohid record start succeeds and prints session_id and output path (either with config default or --output-path)."
  status: failed
  reason: "User reported: Error: start recording failed: Configuration error: Missing required configuration: recording output path. Dispatch to service works; service requires output path in config or --output-path."
  severity: major
  test: 2
  artifacts: []
  missing: []
- truth: "(Previous) With the service running, running neurohid record start succeeds and prints session_id and output path (sends control to existing service, does not start Hub/service)."
  status: failed
  reason: "User reported: running cargo run --bin neurohid record start starts the hub and service (even though the service is already running in another terminal)."
  severity: major
  test: 2
  root_cause: "Hub binary (neurohid.rs) only dispatches to neurohid-service when argv[1] is in CLI_SUBCOMMANDS; 'record' is not in the list (only device, config, pipeline, control, daemon). So 'neurohid record start' etc. never delegate and the Hub starts instead."
  artifacts:
    - path: crates/neurohid/src/bin/neurohid.rs
      issue: "CLI_SUBCOMMANDS omits 'record'"
  missing:
    - "Add 'record' to CLI_SUBCOMMANDS in neurohid.rs so record start/stop/status/export/replay-offline delegate to neurohid-service"
  debug_session: ""
- truth: "neurohid record status (or control snapshot) shows recording_active and current session id when talking to running service."
  status: failed
  reason: "User reported: Same issue as test 2: record status starts the Hub instead of sending control request to running service."
  severity: major
  test: 4
  root_cause: "Same as test 2: 'record' not in CLI_SUBCOMMANDS in neurohid.rs."
  artifacts:
    - path: crates/neurohid/src/bin/neurohid.rs
      issue: "CLI_SUBCOMMANDS omits 'record'"
  missing:
    - "Add 'record' to CLI_SUBCOMMANDS (fixes all record subcommands)"
  debug_session: ""
- truth: "neurohid record export <session_dir> -o out.xdf runs offline and produces a valid .xdf file (no Hub/service start)."
  status: failed
  reason: "User reported: record export starts the Hub instead of running offline export (same pattern as record start/status)."
  severity: major
  test: 6
  root_cause: "Same as test 2: 'record' not in CLI_SUBCOMMANDS in neurohid.rs."
  artifacts:
    - path: crates/neurohid/src/bin/neurohid.rs
      issue: "CLI_SUBCOMMANDS omits 'record'"
  missing:
    - "Add 'record' to CLI_SUBCOMMANDS"
  debug_session: ""
- truth: "neurohid record replay-offline <session_dir> runs offline replay (no Hub start)."
  status: failed
  reason: "User reported: record replay-offline starts the Hub instead of running offline replay (same pattern as record start/status/export)."
  severity: major
  test: 7
  root_cause: "Same as test 2: 'record' not in CLI_SUBCOMMANDS in neurohid.rs."
  artifacts:
    - path: crates/neurohid/src/bin/neurohid.rs
      issue: "CLI_SUBCOMMANDS omits 'record'"
  missing:
    - "Add 'record' to CLI_SUBCOMMANDS"
  debug_session: ""
