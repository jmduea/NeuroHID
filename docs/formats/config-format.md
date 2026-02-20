# System config format

System configuration is versioned so the same setup can be reproduced and compatibility is explicit. This document describes the config file format, version, and compatibility policy in one place.

## Format version

- **Current config format version:** `1`
- The root struct `SystemConfig` includes a top-level `format_version` field (unsigned integer). Readers use it to select the correct deserializer or migration path.
- New and updated config files are written with `format_version: 1`. Existing files that omit the field are treated as version 1 via `#[serde(default)]`.
- The version appears at the top level of the TOML file as `format_version = 1`.

## Compatibility policy

- **Readers** support at least **N = 2** previous format versions. For example, when current version is 3, readers must support versions 2 and 3.
- **Breaking changes** (e.g. removing or renaming required fields) require a new format version. Until external parties depend on this format, breaking changes are acceptable; when they do, document the break and migration steps in this doc or a changelog.
- **Additive changes** (new optional fields with defaults) do not require a new version; use `#[serde(default)]` so older files still deserialize.

## Schema (system config)

Config is stored as TOML. Conceptual shape:

```bnf
SystemConfig   := format_version device signal observation errp decoder
                  recalibration? action storage outlet? service ui?

format_version := integer   ; default 1, current version 1
device          := DeviceConfig
signal          := SignalConfig
observation     := ObservationConfig
errp            := ErrPConfig
decoder         := DecoderConfig
recalibration   := RecalibrationConfig?   ; optional, has defaults
action          := ActionConfig
storage         := StorageConfig
outlet          := OutletConfig?          ; optional
service         := ServiceConfig
ui              := UiConfig?              ; optional
```

Each section (e.g. `[device]`, `[signal]`) corresponds to a subsection of `SystemConfig`. The canonical source of the Rust types is `crates/neurohid-types/src/config.rs` (`SystemConfig` and nested config structs).

## Where the version appears

In the persisted TOML file, `format_version` is the first key at the root level:

```toml
format_version = 1

[device]
# ...
[signal]
# ...
```

Load/save is implemented in `crates/neurohid-storage/src/config.rs`; it serializes and deserializes `SystemConfig` including `format_version`.

## File format: YAML and TOML

Config files can be **YAML** (`.yaml` or `.yml`) or **TOML** (e.g. `.toml`). The same schema and `format_version` apply to both; readers detect format from the file extension. When saving, the serialization format matches the path extension (unknown or missing extension defaults to TOML).

## Pipeline and decoder config scope

The "signal pipeline and decoder" configuration that developers can set via SDK/CLI and this format is scoped as follows.

### DecoderConfig

Decoder (RL policy) configuration:

| Field | Description |
|-------|-------------|
| `model_path` | Path to the decoder model file (e.g. relative to profile directory) |
| `online_learning_enabled` | Whether online learning is enabled |
| `learning_rate` | Learning rate for online updates |
| `gamma` | Discount factor for RL |
| `gae_lambda` | GAE lambda for PPO |
| `update_frequency_steps` | Number of steps between policy updates |
| `batch_size` | Batch size for updates |
| `entropy_coef` | Entropy coefficient for exploration |
| `value_coef` | Value function coefficient |
| `max_grad_norm` | Maximum gradient norm for clipping |

### SignalConfig

Signal preprocessing and feature extraction:

| Field | Description |
|-------|-------------|
| `buffer_size_samples` | Ring buffer size in samples |
| `notch_filter_enabled` | Whether notch filter (powerline) is applied |
| `notch_filter_hz` | Notch frequency (e.g. 50 or 60 Hz) |
| `bandpass_low_hz` | Bandpass low cutoff (Hz) |
| `bandpass_high_hz` | Bandpass high cutoff (Hz) |
| `feature_window_ms` | Feature extraction window (ms) |
| `feature_step_ms` | Feature extraction step (ms); affects output rate |
| `artifact_rejection_enabled` | Whether artifact rejection is enabled |
| `artifact_threshold_uv` | Amplitude threshold for artifact rejection (µV) |

### Recording config

Session recording is configured under `recording` (optional; defaults: no default path, auto off, no caps).

| Field | Description |
|-------|-------------|
| `default_output_path` | Default directory for session folders (path string); omit for no default. |
| `auto_mode` | `off`, `tied_to_runtime`, or `tied_to_output`. When not `off`, recording starts/stops with runtime or output. |
| `max_duration_secs` | Optional cap: stop recording after this many seconds. |
| `max_size_mb` | Optional cap: stop recording when total size reaches this many MB. |

Session folder layout (per phase 4 research) is: `session_<id>/` containing `manifest.json`, `config.yaml` (config snapshot), `profile_meta.json` (or ref when profile active), `streams/` (raw per-stream or combined), and `actions.jsonl` (one JSON object per line: timestamp, action/decision_id, confidence, etc.).
