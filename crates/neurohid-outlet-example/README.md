# neurohid-outlet-example

Minimal outlet extension for NeuroHID. Implements the outlet contract from `neurohid-types`: receives config and channels, runs until shutdown (log-only).

## Build

```bash
cargo build -p neurohid-outlet-example
```

Output (cdylib) is in `target/debug/` or `target/release/`, e.g.:

- Linux: `libneurohid_outlet_example.so`
- Windows: `neurohid_outlet_example.dll`
- macOS: `libneurohid_outlet_example.dylib`

## Load

The runtime discovers extensions from directory paths (default: `<config_root>/extensions`). Each extension lives in a **child directory** containing:

1. `manifest.json` with `name`, `kind: "outlet"`, and optional `library` (filename of the cdylib).
2. The built library file (same name as `library` or the default per platform).

Example manifest:

```json
{"name": "neurohid-outlet-example", "kind": "outlet", "library": "libneurohid_outlet_example.so"}
```

Set `outlet.extension_name` to `"neurohid-outlet-example"` in config so the runtime loads this extension instead of the built-in outlet.

See [Extension contracts and discovery](../../docs/extension-contracts.md) for full details.
