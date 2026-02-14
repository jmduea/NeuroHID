# docs-cache

Documentation cache helper for agents.

## Cache Directory

```
~/.claude/cache/rust-docs/
├── docs.rs/{crate}/{item}.json
├── std/{module}/{item}.json
├── rust-book/{chapter}.json
├── rust-reference/{section}.json
├── cargo-book/{chapter}.json
├── effective-rust/{item}.json
├── releases.rs/{version}.json
├── lib.rs/{crate}.json
└── clippy/{lint}.json
```

## TTL by Source

| Source | TTL | Reason |
|--------|-----|--------|
| std/ | 30 days | Stable |
| rust-book/ | 30 days | Stable, low churn |
| rust-reference/ | 30 days | Normative semantics |
| cargo-book/ | 30 days | Cargo behavior rules |
| effective-rust/ | 30 days | Best-practice reference |
| docs.rs/ | 7 days | Crate updates |
| releases.rs/ | 365 days | Historical |
| lib.rs/ | 1 day | Version changes |
| clippy/ | 14 days | Rust version updates |

## Cache Format

```json
{
  "meta": {
    "url": "...",
    "fetched_at": "2025-01-01T00:00:00Z",
    "expires_at": "2025-01-08T00:00:00Z"
  },
  "content": { ... }
}
```

## Skip Cache

Keywords: refresh, force, --force, update docs

## Degraded Mode

If remote fetch is unavailable:

1. Use freshest unexpired cache entries.
2. If all entries are stale, continue with stale entries but label result as degraded.
3. Prefer Rust Reference/Cargo Book cache for language/build semantics, then Rust Book/Effective Rust for guidance.
