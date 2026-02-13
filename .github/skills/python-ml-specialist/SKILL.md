---
name: python-ml-specialist
description: Evaluate Python ML/deep-learning changes for protocol safety, reproducibility, and quality gates.
user-invocable: true
---

# Skill: python-ml-specialist

## Purpose

Evaluate Python ML/deep-learning changes for reliability and integration safety.

## Inputs

- Changes in Python runtime, training, tests, notebooks.
- Protocol assumptions to/from Rust runtime.

## Checks

1. Runtime protocol assumptions are documented and validated.
2. Reproducibility controls are stated (seed/config/data splits).
3. Notebook experiment path aligns with production path.
4. Quality gates (ruff/black/mypy/tests) are satisfied.
5. All Python commands use `uv` wrappers; no bare `python` commands are introduced.

## Output

- ML integration risk report.
- Required validation steps.
- Blocking gaps before merge.
