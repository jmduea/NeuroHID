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
