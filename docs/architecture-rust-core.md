# Architecture: Rust Core

## Scope

Covers the Rust workspace under `crates/` and `apps/` and the runtime/application binaries.
Each crate's `lib.rs` contains detailed module-level
documentation — this document provides the high-level view.

## Crate Layer Map

```text
┌─────────────────────────────────────────────────────────────┐
│  neuroide (GUI app)    neurohid-service    neurohid-validate │
├─────────────────────────────────────────────────────────────┤
│  neurohid (facade, feature-gated re-exports)                │
├─────────────────────────────────────────────────────────────┤
│  neuroide-hub         neurohid-calibration                  │
│  (GUI screens)        (calibration games/wizard)            │
├─────────────────────────────────────────────────────────────┤
│  neurohid-core (task orchestration + runtime pipeline)      │
├─────────────────────────────────────────────────────────────┤
│  neurohid-device  neurohid-signal  neurohid-platform        │
│  (EEG backends)   (filter+features) (HID emission)         │
│                                                             │
│  neurohid-ipc     neurohid-storage                          │
│  (bridge transport) (encrypted persistence)                 │
├─────────────────────────────────────────────────────────────┤
│  neurohid-types (shared domain types — no internal deps)    │
└─────────────────────────────────────────────────────────────┘
```

Dependencies flow downward. `neurohid-types` sits at the bottom with no
internal dependencies. See [`crate-boundaries.md`](./crate-boundaries.md) for
placement rules and the dependency direction policy.

## Runtime Binaries

| Binary | Purpose |
|---|---|
| `neuroide` | Desktop hub/GUI and management shell |
| `neurohid-service` | Headless long-running service |
| `neurohid-validate` | Soak/latency/boot matrix verification |

## Source Tree

```text
crates/
├── neurohid/              # Feature-gated library facade
├── neurohid-core/          # Task orchestration runtime
├── neurohid-types/         # Shared contracts (config/control/IPC/signal/action)
├── neurohid-device/        # EEG device backends (LSL, Serial, Mock, BrainFlow)
├── neurohid-signal/        # Signal filtering + feature extraction
├── neurohid-platform/      # Cross-platform HID emission
├── neurohid-ipc/           # IPC v3 transport + broker
├── neurohid-storage/       # Encrypted profile/model persistence
├── neurohid-calibration/   # Calibration games + wizard
├── neurohid-service/       # Headless service binary
└── neurohid-validate/      # Validation harness binary
apps/
├── neuroide/               # Desktop GUI app (egui)
└── neuroide-hub/           # Hub library (screens, widgets, workbench)
```

## Hub UI Screens

Dashboard, Devices, Profiles, Calibration, Visualization, Python Lab,
Jupyter IDE, Settings, Stream Console. Uses `egui_dock` for pane docking
and `armas` for consistent component styling.

## Data Layer

- Config: TOML file
- Profiles: JSON metadata
- Model/calibration artifacts: AES-256-GCM encrypted (`*.enc`)
- Key material: OS-native keychain (platform keyring APIs)
- Local-only storage; no cloud persistence required

Schema evolution is driven by versioned Rust types and defaults — no SQL
migration framework.

## Observability

- Structured tracing (`tracing` + `tracing-subscriber`)
- JSON or human-readable output via `NEUROHID_LOG_FORMAT`
- Per-component rate controls via `service.observability`
- Hot-path correlation fields (`decision_id`, `stream_id`)

## Reliability

- Runtime continues operating when the Python bridge is unavailable
- Warn+degrade integrity policy (stream → stage → pipeline escalation)
- Local-only transport reduces external infrastructure dependencies
