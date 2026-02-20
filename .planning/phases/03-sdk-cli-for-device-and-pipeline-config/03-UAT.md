---
status: complete
phase: 03-sdk-cli-for-device-and-pipeline-config
source: 03-01-SUMMARY.md, 03-02-SUMMARY.md
started: "2026-02-20T00:00:00Z"
updated: "2026-02-20T21:00:00Z"
---

## Current Test

[testing complete]

## Tests

### 1. Device list (human)
expected: Running `neurohid device list` or `neurohid-service device list` shows a human-readable table of discovered streams (id, name, type, channels) or a clear message if none; default endpoint 127.0.0.1:47384; -q suppresses progress on stderr.
result: pass

### 2. Device list (JSON)
expected: Running `neurohid device list --json` prints a compact one-line JSON array of streams to stdout (scriptable output).
result: pass

### 3. Device connect by ID
expected: Running `neurohid device connect --device-id <id>` against a running service exits 0 when the stream exists; exits 2 when the stream is not found.
result: issue
reported: "It seems to print that it connected no matter what"
severity: major
fixed: CLI now validates stream id against discovered list before sending ConnectStream; unknown id exits 2 with "stream not found".

### 4. Single entrypoint dispatch
expected: Running `neurohid device list` invokes the neurohid-service binary automatically (no need to run neurohid-service separately); user gets device list output or a connection error if no service is running.
result: pass

### 5. Config show
expected: Running `neurohid config show` (or with `--config <path>`) prints the current system config to stdout as TOML; with `--json` prints compact JSON.
result: pass

### 6. Config validate
expected: Running `neurohid config validate` exits 0 when the config file is valid; exits 3 when invalid or when `--config` path is missing. With `--json`, validation errors are written to stderr as machine-readable JSON.
result: pass

### 7. Pipeline run --dry-run
expected: Running `neurohid pipeline run --dry-run` loads the config and exits 0 if valid, without starting the full runtime (validation only).
result: pass

## Summary

total: 7
passed: 6
issues: 2
pending: 0
skipped: 0

## Gaps

- truth: "device connect --device-id <id> exits 2 when the stream is not found"
  status: failed
  reason: "User reported: prints connected no matter what"
  severity: major
  test: 3
  fixed: CLI now fetches discovered streams and validates --device-id exact match before ConnectStream; unknown id → exit 2.
- truth: "config validate --config <path> accepts --config and exits 3 for missing/invalid file"
  status: failed
  reason: "User reported: unexpected argument '--config' when passed after subcommand"
  severity: major
  test: 6
  fixed: Args: config, profile, json, quiet now have global = true so they are accepted after subcommand.
