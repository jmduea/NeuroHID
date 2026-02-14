# Harness Smoke Report

Generated: 2026-02-14T17:00:14.3088576-06:00

Overall: PASS

## Validator Results

| Check | Status | Details |
| --- | --- | --- |
| routing integrity | PASS | passed |
| routing fixtures | PASS | passed |
| docs contracts | PASS | passed |

## Prompt-to-Route Matrix

| Scenario | Status | Missing Required Agents | Matched Agents |
| --- | --- | --- | --- |
| Docs request | PASS | - | completion-finisher, deep-executor, verifier, writer |
| Architecture review | PASS | - | api-reviewer, architect, completion-finisher, deep-executor, verifier, writer |
| Feature planning | PASS | - | completion-finisher, deep-executor, planner, product-manager, verifier, writer |
| TDD workflow | PASS | - | completion-finisher, deep-executor, test-engineer, verifier, writer |
| UX review | PASS | - | completion-finisher, deep-executor, designer, ux-researcher, verifier, writer |
| Python ML | PASS | - | completion-finisher, deep-executor, scientist, test-engineer, verifier, writer |
| Rust issue | PASS | - | completion-finisher, deep-executor, rust-skill-router, verifier, writer |
| Generic coding task | PASS | - | completion-finisher, deep-executor, verifier, writer |

## Scenario Prompts

- **Docs request**: `Please update docs and changelog for this protocol change`
- **Architecture review**: `Assess architecture and ADR impacts for this migration`
- **Feature planning**: `Do feature planning and define scope for this epic`
- **TDD workflow**: `Apply tdd approach and add a regression test`
- **UX review**: `Run a UX and accessibility review for onboarding`
- **Python ML**: `Review this ML training notebook and inference flow`
- **Rust issue**: `Rust E0502 borrow checker error in Cargo workspace`
- **Generic coding task**: `Implement this refactor and get it ready to commit`
