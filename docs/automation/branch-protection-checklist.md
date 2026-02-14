# Branch Protection Checklist

Use this checklist to enforce branch-first development and block direct pushes to `main`.

## Scope

- Repository: `jmduea/neurohid`
- Protected branch: `main`

## Configure Branch Protection Rule

1. Open **GitHub → Settings → Branches → Add rule**.
2. Branch name pattern: `main`.
3. Enable **Require a pull request before merging**.
4. Enable **Require approvals** (recommended: at least 1).
5. Enable **Dismiss stale pull request approvals when new commits are pushed**.
6. Enable **Require status checks to pass before merging**.
7. Add required checks:
   - `Branch Policy / Enforce PR-only main updates`
   - `CI / Focused Gates`
   - `CI / Test (ubuntu-latest)`
   - `CI / Clippy`
   - `CI / Format`
   - `CI / Documentation`
   - `Docs Freshness / Docs Freshness`
8. Enable **Require branches to be up to date before merging**.
9. Enable **Include administrators**.
10. Save rule.

## Optional Hardening

- Add `Python Quality / Python Quality` as required if Python paths are commonly touched.
- Add `Architecture Gate / Architecture Gate` as required if ADR and architecture metadata are release-critical.

## Verification

1. Open a test PR into `main` and verify all required checks appear.
2. Attempt a direct push to `main` and verify it fails policy (`Branch Policy` job).
3. Merge via PR and verify `main` accepts the update.
