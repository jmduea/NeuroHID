# Project Agents

## Default Project Settings

When creating Rust projects or Cargo.toml files, ALWAYS use:

```toml
[package]
edition = "2024"
rust-version = "1.85"

[lints.rust]
unsafe_code = "warn"

[lints.clippy]
all = "warn"
pedantic = "warn"
```

## Core Capabilities

### 0. Automation Ownership Model

- Shared, reusable Rust intelligence lives in `.github/skills/*`.
- NeuroHID-specific process automation is BMAD-first in `_bmad/neurohid/*` and `_bmad/*/workflows/*`.
- Prompt/runtime hook wiring for NeuroHID-specific behavior lives in `.github/hooks/*`.

### 0.1 Agent Success Defaults

- Use the autonomy harness semantics first for execution tasks with BMAD-native role `deep-executor`.
- Continue implementation loops without pausing for permission between normal sub-steps.
- Stop only for clarification, approval-gated risky/destructive actions, or true no-work-left state.
- Validate incrementally (smallest relevant checks first), then broader checks before handoff.
- Default workflow phases are defined in `_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md`.
- Classify changed-file impact with `.github/scripts/classify-impact.ps1` before selecting gates.
- Use `.github/automation/scope-map.json` as the source of truth for path-to-check/docs routing.
- Prefer canonical local/CI runner `.github/scripts/run-agent-ready-tasks.ps1`.
- Keep architecture index current via `.github/scripts/generate-architecture-index.ps1`.
- Use `.github/scripts/check-docs-freshness.ps1`, `.github/scripts/check-unsafe-compliance.ps1`, and `.github/scripts/verify-protocol-contracts.ps1` as required policy gates.

### 0.2 BMAD ↔ GitHub Boundary

- Replaceable `.github` components MUST be moved to BMAD-owned paths first:
    - Prompt/task logic → `_bmad/*/workflows/*` or `_bmad/*/agents/*`
    - Module-specific process documentation → `docs/automation/*` plus `_bmad/neurohid/*`
- Keep these `.github` components as platform/shared boundaries:
    - `.github/workflows/*` (GitHub Actions platform integration)
    - `.github/hooks/*` (runtime prompt/hook wiring contract)
    - `.github/skills/*` (shared Rust intelligence used by routing)
    - `.github/PULL_REQUEST_TEMPLATE.md` (GitHub UI artifact)
- Keep `.github/automation/scope-map.json` as CI/local routing source of truth unless CI is explicitly migrated to BMAD-native manifest loading.
- For migration details, follow `docs/automation/github-to-bmad-replacement-matrix.md`.

Preferred validation order in this repo:

1. Focused crate checks (for touched Rust crates), e.g. `cargo check -p neurohid-hub`.
2. Cross-crate checks for affected surfaces, e.g. `cargo check -p neurohid-hub -p neurohid-calibration`.
3. Workspace check when changes are broad: `cargo check`.
4. Python quality gates (when Python code changes), using `uv` only.

### 1. Question Routing

Route Rust questions to appropriate skills:

- Ownership/borrowing → m01-ownership
- Smart pointers → m02-resource
- Error handling → m06-error-handling
- Concurrency → m07-concurrency
- Unsafe code → unsafe-checker

Route NeuroHID workflow questions to repo-local assets:

- Documentation freshness + docs updates → BMAD agent `writer`
- Architecture decisions/ADRs → BMAD agents `architect`, `api-reviewer`
- Feature planning → BMAD agents `pm`, `sm`
- TDD/test strategy → BMAD agents `qa`, `tea`, `verifier`
- UX/UI review (app + docs + notebooks) → BMAD agents `ux-designer`, `designer`
- Python/ML & deep learning workflows → BMAD agent `scientist`
- End-of-task hygiene (commit grouping + readiness) → BMAD agent `completion-finisher`
- Continuous execution defaults (no unnecessary waiting) → BMAD agent `deep-executor` + autonomy harness semantics

### Completion Protocol (Required)

For coding tasks, agents must complete this protocol before handoff:

0. Run autonomy execution harness loop and continue until scope is complete or truly blocked.
1. Run writer documentation freshness review and resolve blockers.
2. Confirm README/spec/changelog updates required by the change.
3. Prepare grouped commits by concern (e.g., code, tests, docs, CI).
4. Prepare clear commit messages for each commit group.

### Rust Grounding Policy

When handling Rust design, semantics, unsafe/FFI, and Cargo behavior, use a tiered source strategy:

1. Repo-local skills and existing codebase patterns first.
2. Canonical references for disputed or safety-critical guidance:
    - Rust Book: <https://doc.rust-lang.org/book/>
    - Rust Reference: <https://doc.rust-lang.org/stable/reference/>
    - Cargo Book: <https://doc.rust-lang.org/stable/cargo/>
    - Effective Rust: <https://effective-rust.com/>

Agent outputs should name the source and relevant section when tier-2 escalation is used.

### 2. Code Style

Follow Rust coding guidelines:

- Use snake_case for variables and functions
- Use PascalCase for types and traits
- Use SCREAMING_SNAKE_CASE for constants
- Max line length: 100 characters
- Use `?` operator instead of `unwrap()` in library code

### 3. Error Handling

```rust
// Good: Use Result with context
fn read_config() -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string("config.toml")
        .map_err(|e| ConfigError::Io(e))?;
    toml::from_str(&content)
        .map_err(|e| ConfigError::Parse(e))
}

// Avoid: unwrap() in library code
fn read_config() -> Config {
    let content = std::fs::read_to_string("config.toml").unwrap(); // Bad
    toml::from_str(&content).unwrap() // Bad
}
```

### Python Command Policy

- Use `uv` for all Python execution and tooling commands.
- Do not use bare `python` commands in docs, scripts, or automation.
- Prefer forms such as `uv run --project python ...` (or `uv python --command ...` when needed).

### RTK Command Policy

- Prefer `rtk` as the default proxy for verbose shell commands.
- Always prefix each command in chains as well (e.g., `rtk git add . && rtk git commit -m "msg"`).
- RTK passthrough is safe when no dedicated filter exists.
- Before relying on RTK in a new environment, verify with:
    - `rtk --version`
    - `rtk gain`
- Prefer RTK wrappers for common high-volume output commands:
    - `git` (`status`, `log`, `diff`, `show`, `add`, `commit`, `push`, `pull`)
    - `cargo` (`check`, `build`, `clippy`, `test`)
    - file/search (`ls`, `read`, `grep`, `find`)
    - `gh` (`pr`, `issue`, `run`), `docker`, `kubectl`
- In Copilot/VS Code workflows, prefer hook-routed enforcement where available.
- On Windows + VS Code agent workflows, if command rewrite hooks are not active, use explicit `rtk ...` command prefixes by default.

### 4. Unsafe Code

Every `unsafe` block MUST have a `// SAFETY:` comment:

```rust
// SAFETY: We checked that index < len above, so this is in bounds
unsafe { slice.get_unchecked(index) }
```

### 5. Common Error Fixes

| Error | Cause | Fix |
| ----- | ----- | --- |
| E0382 | Use of moved value | Clone, borrow, or use reference |
| E0597 | Lifetime too short | Extend lifetime or restructure |
| E0502 | Borrow conflict | Split borrows or use RefCell |
| E0499 | Multiple mut borrows | Restructure to single mut borrow |
| E0277 | Missing trait impl | Add trait bound or implement trait |

## Quick Reference

### Ownership

- Each value has one owner
- Borrowing: `&T` (shared) or `&mut T` (exclusive)
- Lifetimes: `'a` annotations for references

### Smart Pointers

- `Box<T>`: Heap allocation
- `Rc<T>`: Reference counting (single-threaded)
- `Arc<T>`: Atomic reference counting (thread-safe)
- `RefCell<T>`: Interior mutability

### Concurrency

- `Send`: Safe to transfer between threads
- `Sync`: Safe to share references between threads
- `Mutex<T>`: Mutual exclusion
- `RwLock<T>`: Reader-writer lock

### Async

```rust
#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async {
        // async work
    });
    handle.await.unwrap();
}
```

## Skill Files

For detailed guidance, see:

- `.github/skills/rust-router/SKILL.md` - Question routing
- `.github/skills/coding-guidelines/SKILL.md` - Code style rules
- `.github/skills/unsafe-checker/SKILL.md` - Unsafe code review
- `.github/skills/m01-ownership/SKILL.md` - Ownership concepts
- `.github/skills/m06-error-handling/SKILL.md` - Error patterns
- `.github/skills/m07-concurrency/SKILL.md` - Concurrency patterns

For NeuroHID custom agent/skill invocation prompts and workflows, see:

- `docs/automation/agent-skill-invocation-playbook.md`
