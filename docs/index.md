# Project Documentation Index

## Project Overview

- Type: monorepo with Rust runtime + Python ML package
- Primary languages: Rust, Python
- Architecture model: local runtime with ML bridge boundary

## Canonical Entry Points

- Product introduction only: [`../README.md`](../README.md)
- Contribution process: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
- Development workflows and CI/automation gates: [`./development-guide.md`](./development-guide.md)
- Deployment and operations workflows: [`./deployment-guide.md`](./deployment-guide.md)
- Python package and ML commands: [`../python/README.md`](../python/README.md)

## Architecture and System Docs

- [Project Overview](./project-overview.md)
- [Source Tree Analysis](./source-tree-analysis.md)
- [Crate Boundaries and Placement Guide](./crate-boundaries.md)
- [Architecture - Rust Core](./architecture-rust-core.md)
- [Architecture - Python ML](./architecture-python-ml.md)
- [Integration Architecture](./integration-architecture.md)
- [API Contracts - Rust Core](./api-contracts-rust-core.md)
- [Data Models - Rust Core](./data-models-rust-core.md)
- [Component Inventory](./component-inventory.md)

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
- Runtime/ML protocol contract: [`./runtime-ml-protocol-v3.md`](./runtime-ml-protocol-v3.md)

## Suggested Reading Order

1. Read [`../README.md`](../README.md) for project purpose and scope.
2. Read [`./project-overview.md`](./project-overview.md) and architecture docs.
3. Use [`./development-guide.md`](./development-guide.md) for local work.
4. Use [`./deployment-guide.md`](./deployment-guide.md) for runtime/ops workflows.
5. Follow the relevant `AGENTS.md` chain for the directory you are changing.
