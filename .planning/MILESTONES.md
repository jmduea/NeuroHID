# Milestones

## v1.0 MVP (Shipped: 2026-02-21)

**Phases completed:** 6 phases, 20 plans

**Key accomplishments:**
1. Versioned contracts and formats — config/profile/stream semantics documented; calibration identity for reproducibility
2. Standalone runtime and control — neurohid-service with control endpoint; CLI snapshot and set-output-enabled
3. SDK/CLI for device and pipeline — device discovery/connection API; config YAML, SDK config API, CLI config/pipeline
4. Standard path and recording — user guide with device→decoder→run path; session recording; XDF export and replay
5. Hub-as-IDE — devices strip, calibration games, training screen, visualization, primary workflow (Run in Hub/background)
6. Composable and extensible — four slot contracts (outlet, device, signal, decoder), extension manifest, name-based loading, example outlet plugin and CI e2e, Extensions screen and CLI list/refresh

**Archived:** [v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md) | [v1.0-REQUIREMENTS.md](milestones/v1.0-REQUIREMENTS.md)

---


## v1.1 Testing, BrainFlow & Framework Separation (Shipped: 2026-02-21)

**Phases completed:** 4 phases, 12 plans

**Key accomplishments:**
1. Framework–Hub separation — Documented framework surface and allowlist; CI enforces Hub/binaries dependency boundary; docs/framework-surface.md and .github/framework-allowlist.toml
2. Thorough testing — Nextest in CI, pipeline integration and extension outlet e2e tests; E2E service + Python client; coverage thresholds (Python 50%, Rust 35%); docs/testing.md for tiers and isolation
3. BrainFlow first-class — docs/brainflow.md; BrainFlow in default build; synthetic board replaces mock in tests/examples/CI; Hub Devices/Settings parity; one runnable example (embedded_runtime)
4. BrainFlow deeper — brainflow-native feature flag and optional native Device/streaming path; BrainFlow 5.13.0 pinned and build order documented; optional scripts/build-brainflow-native.sh; same pipeline as LSL

**Archived:** [v1.1-ROADMAP.md](milestones/v1.1-ROADMAP.md) | [v1.1-REQUIREMENTS.md](milestones/v1.1-REQUIREMENTS.md)

---

