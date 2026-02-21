---
status: complete
phase: 04-standard-path-and-recording
source: 04-01-SUMMARY.md, 04-02-SUMMARY.md, 04-03-SUMMARY.md
started: "2026-02-20T00:00:00Z"
updated: "2026-02-20T00:00:00Z"
---

## Current Test

[testing complete]

## Tests

### 1. Standard path in docs
expected: Docs index links to User guide; user guide has "Standard path: from device to actions" with device→connect→decoder→run walkthrough, informal tone, optional branches.
result: pass

### 2. Start recording via CLI
expected: With the service running, running `neurohid record start` (or `record start --output-path <path>`) succeeds and prints session_id and output path.
result: issue
reported: "running cargo run --bin neurohid record start starts the hub and service (even though the service is already running in another terminal)."
severity: major

### 3. Stop recording via CLI
expected: With a recording in progress, running `neurohid record stop` succeeds and prints session_id.
result: skipped
reason: Impossible to test until the recording start issue (test 2) is fixed.

### 4. Recording status
expected: `neurohid record status` (or control snapshot) shows whether recording is active and the current session id when applicable.
result: issue
reported: "Same issue as test 2: record status starts the Hub instead of sending control request to running service."
severity: major

### 5. Session folder contents
expected: After stopping a recording, the session output directory contains a session folder with manifest.json, a config snapshot, stream data (e.g. streams/ or equivalent), and actions.jsonl.
result: skipped
reason: Blocked by test 2 issues (cannot get real recording session).

### 6. Export session to XDF
expected: Running `neurohid record export <session_dir> -o out.xdf` produces a .xdf file that is valid (e.g. readable by pyxdf or as documented).
result: issue
reported: "record export starts the Hub instead of running offline export (same pattern as record start/status)."
severity: major

### 7. Replay offline
expected: Running `neurohid record replay-offline <session_dir>` runs the pipeline on the recorded session, or the user guide clearly documents how to do so.
result: issue
reported: "record replay-offline starts the Hub instead of running offline replay (same pattern as record start/status/export)."
severity: major

### 8. User guide recording section
expected: The user guide has a "Recording and export" subsection that describes where session folders live, how to export to XDF, and how to open .xdf in EEGLAB/MNE/pyxdf (and replay/replay-offline if applicable).
result: pass

## Summary

total: 8
passed: 2
issues: 4
pending: 0
skipped: 2

## Gaps

- truth: "With the service running, running neurohid record start succeeds and prints session_id and output path (sends control to existing service, does not start Hub/service)."
  status: failed
  reason: "User reported: running cargo run --bin neurohid record start starts the hub and service (even though the service is already running in another terminal)."
  severity: major
  test: 2
  artifacts: []
  missing: []
- truth: "neurohid record status (or control snapshot) shows recording_active and current session id when talking to running service."
  status: failed
  reason: "User reported: Same issue as test 2: record status starts the Hub instead of sending control request to running service."
  severity: major
  test: 4
  artifacts: []
  missing: []
- truth: "neurohid record export <session_dir> -o out.xdf runs offline and produces a valid .xdf file (no Hub/service start)."
  status: failed
  reason: "User reported: record export starts the Hub instead of running offline export (same pattern as record start/status)."
  severity: major
  test: 6
  artifacts: []
  missing: []
- truth: "neurohid record replay-offline <session_dir> runs offline replay (no Hub start)."
  status: failed
  reason: "User reported: record replay-offline starts the Hub instead of running offline replay (same pattern as record start/status/export)."
  severity: major
  test: 7
  artifacts: []
  missing: []
