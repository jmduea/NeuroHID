# Development Guide

## Prerequisites

- Rust `1.85+`
- Python `3.12+`
- `uv` for Python environment and command execution

## Local Setup

```bash
cargo build --workspace
uv sync --directory python
```

## Common Run Commands

```bash
cargo run -p neurohid --bin neurohid
cargo run -p neurohid --bin neurohid-service
uv run --directory python neurohid-ml bridge
```

## Validation and Testing

Rust:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Python:

```bash
uv run --project python pytest python/tests -q
uv run --project python ruff check python/src
uv run --project python black --check python/src
uv run --project python mypy python/src
```

## Automation Scripts

Repository scripts under `.github/scripts/` support focused rust/python/doc/unsafe gates and
architecture-index generation.
