# Project Documentation Index

## Project Overview

- Type: monorepo with Rust runtime + Python ML package
- Primary languages: Rust, Python
- Architecture model: local runtime with ML bridge boundary

## Canonical Entry Points

- Product introduction: [`../README.md`](../README.md)
- Contribution process: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
- **User guide: standard path and workflows:** [User guide](user-guide.md) — one path from device to decoder to actions.
- Development workflows and CI/automation gates: [`./development-guide.md`](./development-guide.md)
- Deployment and operations workflows: [`./deployment-guide.md`](./deployment-guide.md)
- Python package and ML commands: [`../python/README.md`](../python/README.md)

## Architecture and System Docs

- [Crate Boundaries and Placement Guide](./crate-boundaries.md)
- [Architecture - Rust Core](./architecture-rust-core.md)
- [Architecture - Python ML](./architecture-python-ml.md)
- [Integration Architecture](./integration-architecture.md)
- [Protocol and API Reference](./protocol-and-api.md)

## Agent Onboarding Hierarchy

- Root baseline rules: [`../AGENTS.md`](../AGENTS.md)
- Rust workspace guidance: [`../crates/AGENTS.md`](../crates/AGENTS.md)
- Python package guidance: [`../python/AGENTS.md`](../python/AGENTS.md)
- Documentation-specific guidance: [`./AGENTS.md`](./AGENTS.md)

Rule of precedence: root `AGENTS.md` is the baseline; nearest subtree `AGENTS.md` may add or
explicitly override for that subtree.

## Existing References

- Changelog: [`../CHANGELOG.md`](../CHANGELOG.md)
- Crate README files: `../crates/*/README.md`

## Suggested Reading Order

1. Read [`../README.md`](../README.md) for project purpose and scope.
2. Read the architecture docs for system structure.
3. Use [`./development-guide.md`](./development-guide.md) for local work.
4. Use [`./deployment-guide.md`](./deployment-guide.md) for runtime/ops workflows.
5. Follow the relevant `AGENTS.md` chain for the directory you are changing.
