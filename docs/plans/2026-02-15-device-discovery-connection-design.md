# Device Discovery→Connection Design

Date: 2026-02-15  
Status: Approved

## Goal

Solidify a practical, code-anchored mental model for how NeuroHID moves from device discovery to active streaming connection, including interactive and headless runtime modes.

## Scope

### Included

- Discovery → candidate selection → connect → stream → disconnect lifecycle
- Interactive command-driven mode and headless auto-connect mode
- LSL-specific behavior that materially affects connection outcomes
- Troubleshooting by lifecycle stage

### Excluded

- Runtime behavior changes
- New reconnect/session semantics
- Protocol/schema changes

## Canonical Lifecycle

1. Trigger discovery
2. Enumerate candidates
3. Select target stream/device
4. Connect and initialize stream
5. Start and maintain streaming
6. Observe status/health
7. Disconnect and cleanup

## Architecture Overview

- Command ingress arrives from hub actions and control requests.
- Runtime command handling maps to `DeviceCommand` actions.
- `DeviceTask` orchestrates scan/connect/disconnect and stream task lifecycle.
- Backend implementations satisfy `DeviceProvider` (discover/connect) and `Device` (stream/status/disconnect).
- Shared state (`ServiceState`) holds discovered streams and global connection indicators consumed by UI/control paths.

## Code Anchors

- Runtime command/control mapping: `crates/neurohid-core/src/runtime.rs`
- Shared service state and device command channel: `crates/neurohid-core/src/service.rs`
- Discovery/connection orchestration: `crates/neurohid-core/src/tasks/device.rs`
- Provider/device contracts: `crates/neurohid-device/src/traits.rs`
- LSL discovery/connect logic: `crates/neurohid-device/src/lsl/provider.rs`
- LSL streaming/status behavior: `crates/neurohid-device/src/lsl/device.rs`
- External control contract: `crates/neurohid-types/src/control.rs`
- Hub-side command triggers: `crates/neurohid-hub/src/service_manager.rs`

## End-to-End Flow (Interactive Mode)

1. Trigger (`RescanStreams`, `ConnectStream { stream_id }`, `DisconnectStream { stream_id }`) enters runtime.
2. Runtime maps control command to `RuntimeCommand`, then to `DeviceCommand`.
3. `DeviceTask::run_interactive` receives command and:
   - `Rescan`: calls `scan(...)` and refreshes `ServiceState.discovered_streams`.
   - `Connect(stream_id)`: calls `provider.connect(...)`, then spawns a per-stream stream task.
   - `Disconnect(stream_id)`: cancels the task token, awaits join best-effort, then updates state.
4. Stream task executes `device.start_streaming()` loop, forwards samples, periodically updates stream status, and cleans up via `stop_streaming`/`disconnect`.
5. Shared state updates include:
   - per-stream `connected` flag in `discovered_streams`
   - global `device_connected` and `device_name`
   - top-level signal quality/battery-derived fields

### Interactive background behavior

- A 10-second rescan interval exists.
- Rescan is only performed when no streams are active, or no streams are currently discovered.

## End-to-End Flow (Headless Mode)

1. `DeviceTask::run_headless` calls `provider.discover()` once at startup.
2. If no devices are found, task returns `NoDeviceFound`.
3. If devices exist, it connects to the first discovered device.
4. It starts streaming and forwards samples until shutdown/stream end/receiver drop.
5. On exit, it clears global connection state and performs best-effort `stop_streaming` + `disconnect`.

### Headless caveat

- Current behavior is startup discovery + first-device connect; it is not a periodic re-discovery/reconnect loop.

## LSL-Specific Notes

- Naming note: framework-level `DeviceProvider`/`Device` terms are generic.
   In LSL semantics, this effectively means resolver/inlet client.
   (`LslStreamResolver` and `LslInletClient` are provided as aliases.)
- Discovery uses LSL resolution and maps stream metadata into `DeviceInfo` entries.
- Connect path performs a targeted resolve and then stream-id matching before creating inlet/device.
- Ambiguity risk exists when streams share names or have partial metadata; stream-id construction/matching determines final target.

## State Ownership Map

- `ServiceState.discovered_streams`: current candidate list + per-stream connected/quality/battery snapshot
- `ServiceState.device_connected`: global “any stream connected” indicator
- `ServiceState.device_name`: comma-joined active stream IDs (interactive) or single connected ID (headless)
- Device-local status channels (backend-specific): source of battery/quality updates that are folded into shared state

## Failure Model by Lifecycle Stage

### Discovery

- No publishers available / timeout / stale ads
- Observable: empty `discovered_streams` or discovery warnings

### Connect

- Selected stream disappears between scan and connect
- Backend resolve/connect/open-stream failures
- Observable: connect error logs; stream remains disconnected

### Streaming

- Sample read errors, stream termination, downstream receiver closed
- Observable: warning logs, stream task exits, connection flags eventually clear

### Disconnect

- Cancellation races with stream loop
- Best-effort stop/disconnect errors during teardown
- Observable: delayed state reconciliation but eventual disconnected status

## Troubleshooting Checklist

1. Verify command ingress (hub/control) and runtime mapping to `DeviceCommand`.
2. Verify target stream ID exists in `discovered_streams` before connect.
3. For LSL, verify resolved stream metadata actually matches selected stream ID.
4. Verify stream task creation and periodic status updates after connect.
5. Verify disconnect path cancels task and updates both per-stream and global state fields.

## Documentation Freshness Follow-up

- Root README architecture text should describe current backend reality (not legacy Emotiv-only wording).
- Integration architecture should link to this lifecycle document for operator/developer onboarding.

## Out of Scope (Intentional)

- Changing provider semantics for `ConnectionSettings`
- Implementing reconnect policy changes
- Introducing a new device session abstraction
