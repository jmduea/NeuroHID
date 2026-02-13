# Trigger Taxonomy

This taxonomy defines which NeuroHID agent should activate for prompt intent and changed-file context.

## docs-freshness

- Keywords: docs, README, spec, changelog, stale docs, protocol doc
- Typical paths: `docs/**`, `README.md`, `CHANGELOG.md`, protocol docs

## architecture-validator

- Keywords: architecture, ADR, boundary, layering, compatibility, migration
- Typical paths: `crates/neurohid-ipc/**`, `crates/neurohid-storage/**`, core protocol docs

## feature-planner

- Keywords: feature planning, roadmap, epic, milestone, scope, DoR, DoD
- Typical paths: planning docs and multi-crate feature branches

## tdd-enforcer

- Keywords: TDD, tests first, failing test, regression test
- Typical paths: behavior-changing source with missing/insufficient test deltas

## ux-reviewer

- Keywords: UX, UI, usability, accessibility, onboarding
- Typical paths: app UX surfaces, docs tutorials, notebook interaction flows

## python-ml-specialist

- Keywords: ML, deep learning, EEGNet, decoder, ErrP, inference, training
- Typical paths: `python/src/**`, `python/tests/**`, `python/notebooks/**`

## completion-finisher

- Keywords: implement, fix, refactor, finish coding, done coding, ready to commit
- Typical paths: any coding change touching source/docs/workflows
- Required behavior: always run docs-freshness and produce grouped commit plan before final handoff
