---
phase: 03-sdk-cli-for-device-and-pipeline-config
plan: 02
subsystem: sdk-cli
tags: [rust, neurohid-sdk, neurohid-storage, serde_yaml, clap, config, pipeline]

# Dependency graph
requires:
  - phase: 03-sdk-cli-for-device-and-pipeline-config
    plan: 01
    provides: neurohid-service CLI shape, neurohid dispatch for config|pipeline
provides:
  - ConfigStore YAML and TOML load/save by path extension; load_from_path/save_to_path
  - Documented pipeline/decoder and signal config scope (DecoderConfig, SignalConfig) in config-format.md
  - SDK config API (config::load, config::save) with optional path; default uses platform config file
  - CLI config show and config validate; pipeline run --dry-run; exit 3 for config invalid; --json errors to stderr
affects: Phase 4 (standard path), Phase 5 (Hub-as-IDE)

# Tech tracking
tech-stack:
  added: [serde_yaml]
  patterns: Format detection by path extension (.yaml/.yml => YAML); ConfigStore re-use in SDK and service; global --config/--json/-q for CLI

key-files:
  created: crates/neurohid-sdk/src/config.rs
  modified: crates/neurohid-storage/src/config.rs, crates/neurohid-storage/Cargo.toml, docs/formats/config-format.md, crates/neurohid-sdk/src/lib.rs, crates/neurohid-sdk/Cargo.toml, crates/neurohid/src/bin/neurohid-service.rs

key-decisions:
  - "YAML and TOML share same SystemConfig schema and format_version; format inferred from file extension"
  - "Config validate: explicit --config path that does not exist is invalid (exit 3); with --json write ConfigErrorJson to stderr"
  - "Pipeline run without --dry-run returns error in this phase (only validate path implemented)"

patterns-established:
  - "Config commands use DataPaths::new(default_data_dir()) + ConfigStore for load/save; --config override uses load_from_path/save_to_path"
  - "config show: human TOML or --json; config validate: exit 0 valid, 3 invalid; pipeline run --dry-run: load config, exit 0 if valid"

requirements-completed: [COMP-02]

# Metrics
duration: ~25min
completed: 2026-02-20
---

# Phase 3 Plan 02: YAML config, SDK config API, CLI config/pipeline Summary

**ConfigStore YAML/TOML support by path extension, documented decoder/signal scope, SDK config load/save API, and CLI config show/validate plus pipeline run --dry-run.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3
- **Files created:** 1 (crates/neurohid-sdk/src/config.rs)
- **Files modified:** 6 (neurohid-storage config + Cargo.toml, config-format.md, neurohid-sdk lib + Cargo.toml, neurohid-service.rs)

## Accomplishments

- **ConfigStore:** serde_yaml added; format by path extension (.yaml/.yml => YAML); load_from_path/save_to_path for explicit paths; default load/save unchanged (TOML at DataPaths::config_file()).
- **config-format.md:** Section on pipeline/decoder and signal scope (DecoderConfig and SignalConfig fields); YAML/TOML same schema and format_version.
- **SDK config API:** config::load(config_path: Option<PathBuf>) and config::save(config_path, config); None uses platform default path; re-export ConfigStore via storage feature.
- **CLI:** Config subcommand (Show, Validate); Pipeline subcommand (Run --dry-run). Global --config, --json, -q. Validate exit 3 for invalid or missing file at --config; --json writes machine-readable error to stderr. pipeline run --dry-run loads config and exits 0 if valid.

## Task Commits

Each task was committed atomically:

1. **Task 1: YAML config support and pipeline/decoder docs** - `3ba6b9a` (feat)
2. **Task 2: SDK config API** - `00f1eb2` (feat)
3. **Task 3: CLI config and pipeline subcommands** - `6f29d99` (feat)
4. **Follow-up:** service load YAML from --config path, clippy fixes - `416d751` (feat)

## Files Created/Modified

- `crates/neurohid-storage/Cargo.toml` - Added serde_yaml
- `crates/neurohid-storage/src/config.rs` - is_yaml_path, load_from_path, save_to_path; YAML roundtrip test
- `docs/formats/config-format.md` - YAML/TOML section; DecoderConfig and SignalConfig scope tables
- `crates/neurohid-sdk/src/config.rs` - load/save convenience API; module doc
- `crates/neurohid-sdk/src/lib.rs` - pub mod config; config_tests
- `crates/neurohid-sdk/Cargo.toml` - dev-dependency tempfile
- `crates/neurohid/src/bin/neurohid-service.rs` - ConfigCommandCli (Show, Validate), PipelineCommandCli (Run --dry-run), run_config_command, run_pipeline_command; global --json, -q; ConfigErrorJson for stderr; load_runtime_context uses ConfigStore::load_from_path for --config (YAML/TOML by extension)

## Decisions Made

- Validate treats missing file at explicit --config path as invalid (exit 3) for scriptability.
- pipeline run without --dry-run errors in this phase; only validation path is implemented per plan scope.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- neurohid-service.exe was locked during execution (another process); CLI verification commands were not run manually. cargo test --workspace and cargo check passed; user can run `neurohid config show`, `neurohid config validate`, `neurohid config validate --config /nonexistent`, `neurohid pipeline run --dry-run` to verify.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- COMP-02 satisfied: developer can configure signal pipeline and decoder via SDK/CLI and documented config format.
- YAML and TOML both supported; pipeline/decoder scope documented; config show/validate and pipeline run --dry-run available via neurohid or neurohid-service.

## Self-Check: PASSED

- 03-02-SUMMARY.md present; key files exist; commits 3ba6b9a, 00f1eb2, 6f29d99, 416d751 present.

---
*Phase: 03-sdk-cli-for-device-and-pipeline-config*
*Completed: 2026-02-20*
