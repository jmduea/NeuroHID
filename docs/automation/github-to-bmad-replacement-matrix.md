# GitHub-to-BMAD Replacement Matrix

This document defines how to fully replace all applicable `.github` components with BMAD-owned functionality while preserving required GitHub platform integration.

## Classification

### Replaceable (migrate to BMAD)

- `.github/prompts/bmad-*`
  - Target: `_bmad/*/workflows/*` and `_bmad/*/agents/*`
  - Rule: Prompt/task logic is BMAD-owned.
- `.github` process docs tied to BMAD behavior
  - Target: `docs/automation/*` and `_bmad/neurohid/*`
  - Rule: Keep process truth with BMAD module docs.

### Hybrid (keep thin wrappers, move logic)

- `.github/scripts/classify-impact.ps1`
  - BMAD target: `_bmad/neurohid/workflows/neurohid-phase-workflow/*`
  - Rule: wrapper may remain in `.github/scripts`, but decision logic should be BMAD-defined.
- `.github/scripts/generate-architecture-index.ps1`
  - BMAD target: BMAD architecture/documentation workflows
  - Rule: wrapper stays for CI/local compatibility; implementation logic migrates.

### Keep (platform/shared boundary)

- `.github/workflows/*`
  - GitHub Actions event/check integration boundary.
- `.github/hooks/*`
  - Prompt/runtime hook wiring boundary.
- `.github/skills/*`
  - Shared Rust/domain intelligence boundary.
- `.github/PULL_REQUEST_TEMPLATE.md`
  - GitHub UI artifact boundary.
- `.github/automation/scope-map.json`
  - Current CI/local source of truth for path-to-check/docs routing.

## Migration Order

1. Move BMAD prompt/task logic from `.github/prompts/bmad-*` to `_bmad/*`.
2. Ensure BMAD workflows are the source of truth for sequencing and required artifacts.
3. Keep `.github/scripts/*` as stable entry points where CI depends on them; migrate internals gradually.
4. Leave platform/shared boundaries in `.github` unless CI/runtime contracts are redesigned.

## Acceptance Criteria

- BMAD-owned logic no longer depends on `.github/prompts/bmad-*` as a source of truth.
- BMAD workflow docs define canonical execution phases and required artifacts.
- Required GitHub boundaries remain intact and documented.
- Team can identify replaceable vs non-replaceable components from this matrix without ambiguity.
