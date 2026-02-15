# Contribution Guide (Documentation Snapshot)

For authoritative contribution policy, see `CONTRIBUTING.md`.

## High-Level Expectations

- Follow Rust workspace linting and formatting requirements
- Use `uv` for Python tooling and test execution
- Keep docs/changelog updated when behavior or interfaces change
- Respect CI gates and branch policy workflows

## Typical Contributor Workflow

1. Implement scoped changes in the relevant crate/package
2. Run local checks (`cargo test`, `cargo clippy`, `cargo fmt --check`, Python quality tools)
3. Update affected docs in `README.md`, crate READMEs, and `docs/` as needed
4. Submit via pull request targeting `main`

## Safety and Policy Highlights

- Unsafe Rust must include explicit safety rationale comments
- Avoid introducing `unwrap` in library paths where recoverable errors are expected
- Preserve current workspace edition/rust-version and lint baselines
