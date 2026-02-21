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

The **current implementation is simulation/synthetic only**. The BrainFlow adapter in NeuroHID uses a synthetic board (no real BrainFlow C++ SDK). Native SDK integration and real hardware support are planned for **Phase 10** (requirements BRAIN-06–08).

## Build order

- **Phase 9:** No C++ build is required. The default build includes the BrainFlow (synthetic) backend.
- **Phase 10 (native):** When the real BrainFlow SDK is integrated, build order and native dependencies will be documented in the Phase 10 plan and docs.

## Device-agnostic API (BRAIN-04)

BrainFlow is **one backend** behind the existing `DeviceProvider` / `Device` abstraction. The device-agnostic API is preserved: you use the same discovery, connect, and stream APIs regardless of backend. No API changes are required to use BrainFlow.

For stream consumption, timestamps, and latest-sample semantics, see [Stream semantics](formats/stream-semantics.md).

---

**See also:** [User guide](user-guide.md) for the standard path from device to decoder to actions; [framework-surface.md](framework-surface.md) for what to depend on in the Hub and runtime.
