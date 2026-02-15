# Project Documentation Index

## Project Overview

- **Type:** monorepo with 2 primary parts
- **Primary Languages:** Rust, Python
- **Architecture:** hybrid local runtime + ML bridge

## Quick Reference by Part

### Rust Core (`rust-core`)

- **Type:** backend/service + desktop control surface
- **Root:** `crates/`
- **Key binaries:** `neurohid`, `neurohid-service`, `neurohid-validate`

### Python ML (`python-ml`)

- **Type:** data/ML bridge and trainer package
- **Root:** `python/`
- **Key entrypoint:** `neurohid-ml` CLI

## Generated Documentation

- [Project Overview](./project-overview.md)
- [Source Tree Analysis](./source-tree-analysis.md)
- [Architecture - Rust Core](./architecture-rust-core.md)
- [Architecture - Python ML](./architecture-python-ml.md)
- [Integration Architecture](./integration-architecture.md)
- [API Contracts - Rust Core](./api-contracts-rust-core.md)
- [Data Models - Rust Core](./data-models-rust-core.md)
- [Component Inventory](./component-inventory.md)
- [Development Guide](./development-guide.md)
- [Deployment Guide](./deployment-guide.md)
- [Contribution Guide](./contribution-guide.md)

## Existing Documentation in Repository

- [Root README](../README.md)
- [Contributing Guide](../CONTRIBUTING.md)
- [Changelog](../CHANGELOG.md)
- [Root Agent Instructions](../AGENTS.md)
- Crate README files under `../crates/*/README.md`
- Python package README at `../python/README.md`

## Getting Started

1. Read [Project Overview](./project-overview.md)
2. Review [Architecture - Rust Core](./architecture-rust-core.md) and [Integration Architecture](./integration-architecture.md)
3. Use [Development Guide](./development-guide.md) for local build/test/run workflows
4. Use this index as the primary context file for brownfield planning workflows
