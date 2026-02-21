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

### CLI

From the terminal you can list or refresh discovered extensions without starting the Hub:

- **`neurohid extensions list`** — Scan the discovery path and print all extensions (kind, name, path). Exit 0 on success; non-zero if discovery fails (e.g. duplicate extension names).
- **`neurohid extensions refresh`** — Same as `list`: run discovery and print the current set. Use after adding or removing extensions so the Hub or headless runtime can see them on next start (or use the Hub’s Extensions screen Rescan for in-session refresh).

Example output (tab-separated: kind, name, path):

```
outlet	my-custom-outlet	/home/user/.config/neurohid/extensions/my-custom-outlet
```

## Adding an extension

1. **Discovery path:** Install your extension under the default path `<config_root>/extensions` (see [Where the runtime looks](#where-the-runtime-looks)), or configure a custom path when building the registry.
2. **Contract:** Implement the trait for your slot (e.g. `Outlet` in `neurohid-types`, `DeviceProvider` in `neurohid-device`, or the signal/decoder contracts). See [Pipeline slots and contracts](#pipeline-slots-and-contracts).
3. **Manifest:** Add `manifest.json` (or `neurohid.manifest.json`) in the extension directory with `name` and `kind`. See [Extension manifest](#extension-manifest).
4. **Example:** The workspace example outlet `crates/neurohid-outlet-example` and the [Loading extensions](#loading-extensions) section show how to build a cdylib and wire it for discovery.

After adding an extension, run `neurohid extensions list` (or use Hub → Extensions → Rescan) to confirm it is discovered; then select it in Hub Settings for the relevant slot (device backend, signal pipeline, decoder, or outlet).

## Loading extensions

Outlet extensions are loaded by `neurohid-core` via `ExtensionRegistry::load_outlet`. The runtime expects a cdylib that exports the symbol `neurohid_outlet_create` with signature `(OutletConfig, OutletChannels) -> Result<Box<dyn Outlet>>`. In-process plugins must be built with the same Rust toolchain as the host (ABI). See `crates/neurohid-core/src/extension_registry.rs` for the loader.

### Example outlet plugin

The workspace includes an example outlet: `crates/neurohid-outlet-example`. It implements the outlet contract (minimal run-until-shutdown behaviour). To build and use it:

- **Build:** `cargo build -p neurohid-outlet-example` (produces a cdylib in `target/debug/` or `target/release/`, e.g. `libneurohid_outlet_example.so` on Linux).
- **Manifest:** In the extension directory (a child of a path passed to the registry), place `manifest.json` with `{"name": "neurohid-outlet-example", "kind": "outlet", "library": "<filename>"}`. The `library` value must match the built artifact (e.g. `libneurohid_outlet_example.so` on Linux, `neurohid_outlet_example.dll` on Windows).
- **Discovery:** Point the extension registry at a directory that contains a child directory with that manifest and the copied library, or use the default `<config_root>/extensions` and install the extension there.

## References

- Contracts and types: `crates/neurohid-types` (outlet, signal_contract, decoder_contract).
- Device contract: `crates/neurohid-device` (`DeviceProvider`).
- Registry and default path: `crates/neurohid-core/src/extension_registry.rs`.
