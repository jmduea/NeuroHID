# Trigger Taxonomy

This taxonomy defines which NeuroHID agent should activate for prompt intent and changed-file context.

- Routing contract version: `2026-02-v1`

Default multi-agent flow for all prompts:

- `.github/agents/deep-executor.md`
- `.github/agents/verifier.md`
- `.github/agents/writer.md`
- `.github/agents/completion-finisher.md`

Canonical workflow reference:

- `.github/agents/_shared/multi-agent-phase-workflow.md`

## writer

- Keywords: docs, README, spec, changelog, stale docs, protocol doc, migration notes
- Typical paths: `docs/**`, `README.md`, `CHANGELOG.md`, protocol docs
- Required behavior: own docs freshness verdict and required update checklist

## architect + api-reviewer

- Keywords: architecture, ADR, boundary, layering, compatibility, migration
- Typical paths: `crates/neurohid-ipc/**`, `crates/neurohid-storage/**`, core protocol docs

## planner + product-manager

- Keywords: feature planning, roadmap, epic, milestone, scope, DoR, DoD
- Typical paths: planning docs and multi-crate feature branches

## test-engineer + verifier

- Keywords: TDD, tests first, failing test, regression test
- Typical paths: behavior-changing source with missing/insufficient test deltas

## ux-researcher + designer

- Keywords: UX, UI, usability, accessibility, onboarding
- Typical paths: app UX surfaces, docs tutorials, notebook interaction flows

## scientist

- Keywords: ML, deep learning, EEGNet, decoder, ErrP, inference, training
- Typical paths: `python/src/**`, `python/tests/**`, `python/notebooks/**`

## completion-finisher

- Keywords: implement, fix, refactor, finish coding, done coding, ready to commit
- Typical paths: any coding change touching source/docs/workflows
- Required behavior: verify writer docs freshness output, then produce grouped commit plan before final handoff

## rust-skill-router

- Keywords: rust, cargo, compiler diagnostics, ownership/borrow/lifetimes, unsafe/ffi
- Typical paths: `crates/**`, `Cargo.toml`, Rust-related docs
- Required behavior: route Rust prompts through `rust-router` and specialized Rust skills
