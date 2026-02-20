# NeuroHID User Guide

This guide walks you from having a device in hand to running a decoder that drives actions. One path, minimal assumptions — with optional branches if you do things a bit differently.

## Standard path: from device to actions

Here’s the single path that gets you from hardware to actions: **pick a device → pick a decoder (config + profile) → run**. Everything else (transports, control, observability) is optional or linked from the [deployment guide](deployment-guide.md).

### 1. Device in hand

You have a biosignal device (e.g. EEG headset) that NeuroHID can talk to. If you’re using LSL, make sure the stream is visible to the runtime; if you’re using a direct driver or mock, ensure the service can discover it.

### 2. Pick and connect the device

**CLI:** With the service running (or using the `neurohid` entrypoint that forwards to it), list and connect:

- `neurohid device list` — human-readable table of discovered streams (id, name, type, channels).
- `neurohid device list --json` — one-line JSON for scripts.
- `neurohid device connect --device-id <id>` — connect to a stream by id; use `--criteria` if you prefer matching by name/type.

If the stream isn’t found, the connect command exits with code 2. Default endpoint is `127.0.0.1:47384` unless you override it.

**SDK:** From code, use the neurohid-sdk device API: list streams, then `connect_by_id` or `connect_by_criteria`; you get a handle that stays valid until disconnect. The [deployment guide](deployment-guide.md) describes transport and control endpoints if you need them.

### 3. Pick a decoder (config + profile)

The decoder is defined by your **config** and **profile**: config holds system-wide and pipeline/decoder settings, and the profile names the decoder model and calibration identity. No separate “attach decoder” step — the profile implies the decoder.

- **Config:** YAML or TOML by file extension. See [config format](formats/config-format.md) for schema (decoder path, signal pipeline, etc.). Validate before running:
  - `neurohid config show` — print current config (optionally `--config <path>`).
  - `neurohid config validate` — exit 0 if valid, 3 if invalid; use `--json` for machine-readable errors on stderr.
- **Pipeline:** To check that the full pipeline (including decoder path) loads without starting the runtime: `neurohid pipeline run --dry-run` (exits 0 when valid).

*Advanced:* Override config path with `--config <path>`; use a profile with `--profile <name>` when starting the service.

### 4. Run

**Service (typical):** Start the standalone runtime so it loads your config and profile and runs the decoder:

```bash
cargo run --release -p neurohid --bin neurohid-service
```

Optional: `--config <path>`, `--profile <name>`, `--control-port <port>`. The decoder is loaded from the profile; device connection can be done beforehand via CLI/SDK (e.g. `neurohid device connect`) or via the Hub. For install/start/stop as a Windows service and other ops, see the [deployment guide](deployment-guide.md).

**Pipeline run:** You can run the pipeline (with or without `--dry-run` for validation only). Without `--dry-run`, the pipeline starts the runtime; defaults apply for config/profile if not overridden.

**If you use LSL:** Same path — discover streams (device list will show LSL streams when the service is configured for it), connect, then run with your config and profile. Transport and IPC details are in the [deployment guide](deployment-guide.md).

### Optional branches

- **Hub GUI:** You can use the NeuroHID Hub to pick device and profile and start/stop; the same “device → decoder (config + profile) → run” flow applies.
- **Control without Hub:** Status and output toggle are available via `neurohid-service control snapshot` and `neurohid-service control set-output-enabled`; see [deployment guide](deployment-guide.md).
- **Advanced config:** Override config path, control port, or IPC endpoint via config file or flags as documented in the deployment guide.

## Recording and export

Session folders are written when you start recording (via control/CLI or Hub). Each session has a folder (e.g. `session_<id>`) containing `manifest.json`, `config.json`, `streams/*.jsonl`, and `actions.jsonl`. The default location is from your recording config; you can override it when starting a recording.

**Export to XDF:** To get a single file for use in common tools, export a session folder to XDF 1.0 (offline; no running service needed):

```bash
neurohid-service record export <path/to/session_folder> -o out.xdf
```

Exported `.xdf` files open in **EEGLAB**, **MNE-Python** (`mne.io.read_raw_xdf()`), and **Python** with **pyxdf** (`pyxdf.load_xdf()`). Session layout and stream semantics are documented in the config and recording format docs.

**Replay:** You can run the pipeline on a recorded session instead of a live device (replay mode). Use `--replay <path/to/session_folder>` when starting the service, or run an offline pass with:

```bash
neurohid-service record replay-offline <path/to/session_folder>
```

Replay feeds the session's streams through the same signal and decoder path so you can validate or compare outputs.

---

In short: **device in hand → list/connect device (CLI or SDK) → set config and profile → run service or pipeline.** For transport, control, observability, and validation harness, use the [deployment guide](deployment-guide.md).
