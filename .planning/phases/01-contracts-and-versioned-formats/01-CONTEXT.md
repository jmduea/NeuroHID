# Phase 1: Contracts and versioned formats - Context

**Gathered:** 2025-02-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Config, profile, and stream semantics are versioned and documented so the same setup can be reproduced. Profile and config formats have a documented version and compatibility policy; stream consumption, timestamps, and "latest sample" semantics are documented; calibration and profile metadata are stored with version/identity for reproducibility. No new capabilities—this phase only clarifies how to implement what is already scoped.
</domain>

<decisions>
## Implementation Decisions

### Version and compatibility policy
- Compatibility policy lives in the same doc as the format spec (one place for version + rules).
- Until other people depend on this, breaking changes are acceptable; when they do, document breaks and migration.
- Support reading at least N previous format versions (N to be chosen during implementation/planning).

### Reproducibility identity
- Use case for "same setup can be reproduced": both re-run with same config and audit/share (others can verify or reuse the exact setup).

### Stream semantics documentation
- Document for streams: consumption model, timestamps, and ordering/drops (overflow/drop behavior).
- Documentation style: structured — BNF/schema plus brief prose.

### Claude's Discretion
- Versioning scheme for profile/config (e.g. semver vs single integer).
- What exactly identifies "same setup" (version-only vs version + content hash).
- Whether reproducibility applies to calibration, profile, or both in this phase.
- Where identity is stored (in-file, alongside, or both).
- Which stream types get documented semantics (LSL only vs all current stream types).
- Whether "latest sample" is defined once (generic) or per stream type.
</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches.
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.
</deferred>

---

*Phase: 01-contracts-and-versioned-formats*
*Context gathered: 2025-02-20*
