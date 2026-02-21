# BrainFlow with NeuroHID

Canonical reference for using the BrainFlow backend with NeuroHID: setup, configuration, synthetic vs native hardware, and build order.

## Setup

BrainFlow is **included in the default build** for the Hub binary and examples that depend on `neurohid-core`. You do not need custom build flags to select the BrainFlow backend in Hub Settings or to run examples.

If you build `neurohid-core` without default features (e.g. `default-features = false`), enable the optional feature to get BrainFlow:

```toml
neurohid-core = { path = "...", features = ["brainflow"] }
```

## Config

- **Backend:** Set `backend` to `BrainFlow` in device config (e.g. in Hub Settings or in config files).
- **Board and port:** Use the `brainflow` section of device config. The type is `BrainFlowConfig` in `neurohid-types`:
  - **`board_id`** — Default `0` = Synthetic (simulation). Other IDs (e.g. 1 = Cyton, 2 = Ganglion) are for metadata/native use; see Phase 10 for real hardware.
  - **`serial_port`** — Optional; used when connecting to a real board (Phase 10).

## Synthetic vs native

The **current implementation is simulation/synthetic only**. The BrainFlow adapter in NeuroHID uses a synthetic board (no real BrainFlow C++ SDK). For real hardware, use the **Native SDK (Phase 10)** path below; it is optional and not used in the default or CI build.

## Build order

- **Phase 9 (default):** No C++ build is required. The default build includes the BrainFlow (synthetic) backend.
- **Phase 10 (native):** See **Native SDK (Phase 10)** below for reproducible C++ → Rust → neurohid-device build order when using the real BrainFlow SDK.

### Native SDK (Phase 10)

NeuroHID pins the BrainFlow version and documents build steps so that native builds are **reproducible** (requirement BRAIN-08). Default and CI do **not** use the native SDK; they use the synthetic backend only.

**Pinned version**

- Use the **BrainFlow git tag `5.13.0`** (or the exact commit that includes both the C++ core and the in-tree `rust_package/brainflow`). NeuroHID documents this tag for reproducible builds. Check the [BrainFlow releases](https://github.com/brainflow-dev/brainflow/releases) for the current recommended tag if you need a different version.

**Build order (authoritative)**

1. **C++** — From the BrainFlow repo root (clone at the pinned tag):
   - Run the official build script: `python tools/build.py` (or `uv run python tools/build.py` if using uv). Optional args: `--brainflow-version`, `--cmake-install-prefix`, `--build-dir` as needed.
   - Output goes to `installed/lib/` (BoardController, DataHandler, MLModule shared libs) and `installed/inc/` (headers).

2. **Rust** — The official Rust binding (`rust_package/brainflow` in the BrainFlow repo) expects a `lib/` directory **inside** the crate. After building C++:
   - Copy `installed/lib/*` into `rust_package/brainflow/lib/` (inside the BrainFlow repo), then build the Rust crate from that tree. Alternatively, if the crate is patched to support an env-based path, set e.g. `BRAINFLOW_LIB_DIR` to `installed/lib` and build from the crate directory.

3. **NeuroHID** — Build the device crate with the native feature (and `brainflow` if not already default):
   - `cargo build -p neurohid-device --features brainflow,brainflow-native`
   - Default and CI do **not** use `brainflow-native`; only use it when you have built the C++ and Rust stack above.

**Optional script**

An optional script `scripts/build-brainflow-native.sh` can run the C++ build and copy libs to a target directory for a reproducible native stack. Manual steps in this section are sufficient; the script is for convenience. See that script and the doc references there for Windows (e.g. WSL or manual steps).

## Device-agnostic API (BRAIN-04)

BrainFlow is **one backend** behind the existing `DeviceProvider` / `Device` abstraction. The device-agnostic API is preserved: you use the same discovery, connect, and stream APIs regardless of backend. No API changes are required to use BrainFlow.

For stream consumption, timestamps, and latest-sample semantics, see [Stream semantics](formats/stream-semantics.md).

---

**See also:** [User guide](user-guide.md) for the standard path from device to decoder to actions; [framework-surface.md](framework-surface.md) for what to depend on in the Hub and runtime.
