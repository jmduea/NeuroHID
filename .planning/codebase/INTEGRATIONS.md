# External Integrations

**Analysis Date:** 2026-02-20

## APIs & External Services

**None.** The application does not call external REST/GraphQL APIs or third-party SaaS backends. All runtime communication is local (IPC, LSL).

**Local / protocol integrations:**
- **Lab Streaming Layer (LSL)** — Consumes EEG streams from LSL-compatible hardware or apps on the same machine/network. Optional feature `device-lsl` in `crates/neurohid-core`, `crates/neurohid-device`; `lsl` crate and patched `lsl-sys` from `https://github.com/labstreaminglayer/liblsl-rust.git`. Documentation: `crates/neurohid-device/src/lib.rs`, `docs/architecture-rust-core.md`.
- **Jupyter Lab** — Optional local IDE; default URL `http://127.0.0.1:8888/lab` and launch command in `crates/neurohid-types/src/config.rs`, `crates/neurohid-hub/src/screens/settings.rs`. No external Jupyter hosting.

## Data Storage

**Databases:**
- None. No PostgreSQL, SQLite, or other database client in the repo.

**File Storage:**
- Local filesystem only. Profile and config data under platform app dirs via `dirs`; secure storage uses platform keychain (keyring) + local encrypted files (aes-gcm) in `crates/neurohid-storage`.

**Caching:**
- None. No Redis or other cache service.

## Authentication & Identity

**Auth Provider:**
- Custom / none. No OAuth, OIDC, or third-party identity provider. No user accounts; access is local machine and process-bound. Secure storage uses platform keychain (keyring) and local encryption in `crates/neurohid-storage`.

## Monitoring & Observability

**Error Tracking:**
- None. No Sentry, Rollbar, or similar. Errors surface via tracing logs and in-process responses.

**Logs:**
- Structured tracing to stdout (JSON or text via `NEUROHID_LOG_FORMAT`); filter with `RUST_LOG`. Implemented in `crates/neurohid/src/tracing_init.rs`, `docs/deployment-guide.md`.

**Coverage (CI only):**
- Codecov via `codecov/codecov-action@v5` in `.github/workflows/ci.yml` and `.github/workflows/python-quality.yml`; uploads Rust lcov and Python coverage XML. No runtime monitoring service.

## CI/CD & Deployment

**Hosting:**
- Not specified. Deployment is local/desktop or headless service; see `docs/deployment-guide.md`. No Heroku, Vercel, or cloud app platform in repo.

**CI Pipeline:**
- GitHub Actions. Workflows: `.github/workflows/ci.yml` (Rust + Python tests, clippy, fmt, docs, IPC compat matrix, coverage, unsafe compliance, protocol contracts), `.github/workflows/python-quality.yml`, `.github/workflows/architecture-gate.yml`, `.github/workflows/release.yml`, `.github/workflows/branch-policy.yml`. Runners: self-hosted (linux, windows, macos, neurohid-ci). No external CI provider beyond GitHub.

## Environment Configuration

**Required env vars:**
- None mandatory for normal run. Optional: `NEUROHID_LOG_FORMAT`, `RUST_LOG`, `NEUROHID_SERVICE_BIN` (validation), `NEUROHID_NOTIFY_TITLE` / `NEUROHID_NOTIFY_BODY` (notifications), WSL: `WSL_DISTRO_NAME`, `WSLENV`, `WINIT_UNIX_BACKEND`. CI: `PYTHON_COVERAGE_MIN`, `RUST_COVERAGE_MIN`, `CARGO_TERM_COLOR`, `RUST_BACKTRACE`.

**Secrets location:**
- Platform keychain (keyring) for profile secrets; no cloud secrets manager. Repo does not read `.env` or credential files (see forbidden files policy).

## Webhooks & Callbacks

**Incoming:**
- None. No HTTP endpoints for external webhooks.

**Outgoing:**
- None. No outbound webhook or callback URLs to external services.

---

*Integration audit: 2026-02-20*
