---
status: complete
phase: 06-composable-and-extensible
source: 06-01-SUMMARY.md, 06-02-SUMMARY.md, 06-03-SUMMARY.md, 06-04-SUMMARY.md
started: "2026-02-21T00:00:00Z"
updated: "2026-02-21T00:00:00Z"
---

## Current Test

[testing complete]

## Tests

### 1. Extension contracts documentation
expected: docs/extension-contracts.md exists and describes the four slot contracts, manifest format, and discovery path; docs index links to it.
result: pass

### 2. Runtime snapshot shows slot names
expected: When requesting runtime status (e.g. control snapshot via CLI or service), the response includes device_name, outlet_name, signal_name, and decoder_name for the active pipeline.
result: pass

### 3. Example outlet builds and e2e passes
expected: cargo build --workspace succeeds; cargo test -p neurohid-core --test extension_outlet_e2e passes.
result: pass

### 4. Hub Extensions screen
expected: In the Hub, the Extensions screen lists discovered extensions by kind (outlet, device, signal, decoder) and has a Rescan control; when no extensions exist, the same UI shows with an empty or shorter list.
result: pass
note: "User does not prefer the Config → Extensions route; consider Extensions as top-level lane or more prominent entry in a future iteration."

### 5. Hub Settings slot dropdowns
expected: In Settings, device backend dropdown shows built-in options plus discovered device extension names; signal, decoder, and outlet dropdowns show Built-in plus extension names; changing a selection persists to config immediately (no need to open another screen).
result: pass

### 6. CLI extensions list and refresh
expected: Running neurohid extensions list prints discovered extensions (e.g. kind, name, path) and exits 0; neurohid extensions refresh rescans and prints; discovery failure exits non-zero.
result: pass

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0

## Gaps

[none — Test 4 passed after sidebar fix; user note: prefer not Config→Extensions route for future UX]
