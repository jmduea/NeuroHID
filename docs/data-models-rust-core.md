# Data Models and Persistence (`rust-core`)

## Core Domain Model Groups

Shared types in `neurohid-types` define:

- System configuration model (device, signal, decoder, service, storage, UI)
- Control protocol requests/responses and snapshots
- Signal and feature representations (samples, vectors)
- Action/output models (HID actions)
- Profile, model manifest, and training-related metadata

## Persistence Layout

Local, user-scoped storage includes:

- Config file (TOML)
- Profile metadata (JSON)
- Encrypted model/calibration artifacts (`*.enc`)
- Key material in OS-native keychain/credential store

## Security Model

- At-rest encryption for sensitive model/profile payloads (AES-GCM pattern)
- Master key lifecycle tied to platform keyring APIs
- Local-only storage assumptions; no cloud persistence required by default

## Migration Notes

No explicit SQL migration framework or database migration directory was detected in the primary
runtime scan; config/profile evolution appears schema-driven via versioned Rust types and defaults.
