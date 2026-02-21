# Extension contracts and discovery

This document describes the four pipeline-slot contracts and how extensions are discovered and registered. It is the canonical reference for building or integrating pluggable components (acquisition, signal preprocessing, decoder, output).

## Pipeline slots and contracts

The runtime has four swappable slots. Each has a published contract (trait or equivalent) so that built-in and extension implementations can be used interchangeably. Config is the source of truth for which implementation is selected (built-in by enum or extension by name).

### 1. Outlet (output / effector)

- **Contract:** `neurohid_types::Outlet`. Accepts outlet config and four broadcast channels (sample, feature, action, marker); runs until shutdown.
- **Types:** `OutletConfig`, `OutletChannels`, `Sample`, `FeatureVector`, `Action`, `StreamMarker` from `neurohid-types`.
- **Built-in:** LSL and TCP/JSON outlets in `neurohid-core` (`OutletTask`).

### 2. Device (acquisition)

- **Contract:** `DeviceProvider` in `neurohid-device`. Handles discovery and connection; provides a sample stream.
- **Built-in:** Mock, LSL, Serial, BrainFlow in `neurohid-device`.

### 3. Signal preprocessing

- **Contract:** `neurohid_types::SignalPreprocessor`. Consumes raw samples, produces feature vectors (and optionally forwards samples/markers). Runs until shutdown.
- **Types:** `SignalConfig`, `Sample`, `FeatureVector`, `StreamMarker` from `neurohid-types`.
- **Built-in:** Filter + feature pipeline in `neurohid-core` (`SignalTask`).

### 4. Decoder

- **Contract:** `neurohid_types::DecoderRunner`. Consumes feature vectors, produces actions (with profile/model loading as needed). Runs until shutdown.
- **Types:** `DecoderConfig`, `FeatureVector`, `Action`, `ProfileId` from `neurohid-types`.
- **Built-in:** ONNX decoder in `neurohid-core` (`DecoderTask`).

## Extension manifest

Each extension is described by a **manifest** (e.g. `manifest.json` or `neurohid.manifest.json` in the extension directory). The manifest is JSON and must include:

- **`name`** (string): The sole identifier for the extension. No version in the ID; duplicate names across discovered extensions are an error.
- **`kind`** (string): One of `outlet`, `device`, `signal_preprocessing`, `decoder`. Used by the registry to list extensions per slot.

Example:

```json
{
  "name": "my-custom-outlet",
  "kind": "outlet"
}
```

Rust type: `neurohid_types::ExtensionManifest` with `ExtensionKind` enum.

## Discovery

### Where the runtime looks

- **Default:** One directory derived from the platform config root:
  - Same root as storage: `neurohid_storage::default_data_dir()` (e.g. `~/.config/neurohid` on Linux, `~/Library/Application Support/neurohid` on macOS, `%APPDATA%\neurohid` on Windows).
  - Extensions subdirectory: `<config_root>/extensions`.
- **Override:** The application can pass a custom list of directory paths when constructing the extension registry (e.g. from config or env). Override replaces or extends the default depending on application policy; document your override in config or env docs.

### How scanning works

- The **extension registry** (`neurohid_core::ExtensionRegistry`) is built with a list of paths. Call `scan()` to refresh.
- For each path, the registry lists direct **child directories**. For each child directory it looks for a manifest file named `manifest.json` or `neurohid.manifest.json`. If found, it parses the manifest and records the extension by name and kind.
- **Duplicate names:** If the same `name` appears in more than one manifest (across any path), `scan()` returns an error (`ExtensionError::DuplicateName`) and the registry does not start with a partial set. There is no silent deduplication.
- **Rescan:** Discovery runs at startup. Explicit refresh (e.g. from Hub or CLI) is supported so new extensions appear without a full process restart.

### List methods

After a successful `scan()`, the registry exposes:

- `list_outlets()` — extensions with `kind: outlet`
- `list_devices()` — extensions with `kind: device`
- `list_signal_preprocessors()` — extensions with `kind: signal_preprocessing`
- `list_decoders()` — extensions with `kind: decoder`

Each returns a list of entries (name + path to the extension directory) for use by Hub/CLI and by factories that resolve a selected name to an implementation (loading is a separate step; see plan 02).

## Loading extensions

Loading (e.g. dynamic library or subprocess) is not covered in this document; it is implemented in a later plan. This document defines only the contracts and discovery behaviour.

## References

- Contracts and types: `crates/neurohid-types` (outlet, signal_contract, decoder_contract).
- Device contract: `crates/neurohid-device` (`DeviceProvider`).
- Registry and default path: `crates/neurohid-core/src/extension_registry.rs`.
