# Harness Smoke Report

Generated: 2026-02-14T14:43:39.4424962-06:00

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
| Docs request | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/verifier.md, .github/agents/writer.md |
| Architecture review | PASS | - | .github/agents/api-reviewer.md, .github/agents/architect.md, .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/verifier.md, .github/agents/writer.md |
| Feature planning | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/planner.md, .github/agents/product-manager.md, .github/agents/verifier.md, .github/agents/writer.md |
| TDD workflow | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/test-engineer.md, .github/agents/verifier.md, .github/agents/writer.md |
| UX review | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/designer.md, .github/agents/ux-researcher.md, .github/agents/verifier.md, .github/agents/writer.md |
| Python ML | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/scientist.md, .github/agents/test-engineer.md, .github/agents/verifier.md, .github/agents/writer.md |
| Rust issue | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/rust-skill-router.md, .github/agents/verifier.md, .github/agents/writer.md |
| Generic coding task | PASS | - | .github/agents/completion-finisher.md, .github/agents/deep-executor.md, .github/agents/verifier.md, .github/agents/writer.md |

## Scenario Prompts

- **Docs request**: `Please update docs and changelog for this protocol change`
- **Architecture review**: `Assess architecture and ADR impacts for this migration`
- **Feature planning**: `Do feature planning and define scope for this epic`
- **TDD workflow**: `Apply tdd approach and add a regression test`
- **UX review**: `Run a UX and accessibility review for onboarding`
- **Python ML**: `Review this ML training notebook and inference flow`
- **Rust issue**: `Rust E0502 borrow checker error in Cargo workspace`
- **Generic coding task**: `Implement this refactor and get it ready to commit`
