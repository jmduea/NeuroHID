# Phase 3: SDK/CLI for device and pipeline config - Context

**Gathered:** 2026-02-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Developer can drive device discovery, connection, and stream selection via public SDK (Rust) and/or CLI, and can configure the signal pipeline and decoder (e.g. model path, params) via SDK/CLI and a documented config format. Scope is fixed by the roadmap; discussion clarifies how to implement within this boundary.

</domain>

<decisions>
## Implementation Decisions

### Discovery and connection flow
- **List + connect-by-id and connect-by-criteria:** Support both: list devices then connect by id for control; optional connect-by-criteria (e.g. first LSL stream) for quick scripts.
- **Ongoing discovery:** Discovery runs in the background; SDK exposes an updating list or notifies when devices appear/disappear.
- **Observing changes:** Both: optional listener/callback for reactive UIs, plus a way to get the current device list for scripts.
- **Criteria ambiguity:** When connecting by criteria, use first match; document that order is implementation-defined.
- **Multiple connections:** Multiple simultaneous connections (e.g. multiple devices or streams) are in scope; API should allow it.
- **Device disconnect:** When a device disappears, SDK notifies (callback or status change) and can invalidate the handle so further calls fail fast.
- **Handle lifecycle:** Handles are ref-counted or scoped (drop = disconnect); explicit disconnect optional but allowed.

### CLI shape and output
- **Structure:** Subcommands per concern (e.g. `neurohid device list`, `neurohid device connect`, `neurohid config set`, `neurohid pipeline ...`). Optional Hydra-style config for composition/overrides.
- **List output:** Default human-readable table; `--json` for scriptable output.
- **Verbosity:** Default normal output; `-v`/`--verbose` for more; `-q`/`--quiet` for less.
- **Config source:** Config file as main source; flags override (e.g. `--config path`, `--profile path` overrides file).
- **Exit codes:** Conventional: 0 = success, non-zero = failure; document specific codes per error type (e.g. 1 = generic, 2 = not found, 3 = config invalid).
- **Progress:** Spinner or progress on stderr, result on stdout; `-q` suppresses progress.
- **Help:** Short one-line summary per subcommand in main help; full description in man/docs or `neurohid <cmd> --help`.
- **Interactive prompts:** Prompt when ambiguous (e.g. multiple devices and no `--device-id` → prompt to choose); otherwise no prompts so scripts/CI work when args are explicit.
- **JSON style:** Compact one-line JSON by default for `--json`; optional `--json pretty` or `-v` for indented.
- **Dry run / validate:** Support dry run or validate-only (e.g. `neurohid pipeline run --dry-run` or `neurohid config validate`) to check config without starting.

### Config format and scope
- **Pipeline/decoder scope:** Decoder (model path, params) plus signal preprocessing options (e.g. filter bounds, reference channel) where the stack exposes them.
- **Config file format:** Support both YAML and TOML.

### Errors and status
- **Machine-readable CLI errors:** When `--json` is in effect, failures write a JSON object (e.g. to stderr) with code, message, and optional details for scripts.
- **Status surface:** Richer than minimum: include pipeline-stage health (e.g. signal path ok, decoder output rate) where the stack supports it; extend or align with Phase 2 runtime status as needed.

### Claude's Discretion
- **Discovery/connection:** Sync vs async for connect; stream selection approach (list then choose vs type-based first match); whether stream selection is part of connect or a separate step; device identity (stable id + name, id canonical for API); whether connection handle represents device-only or device+stream.
- **CLI:** Whether CLI is thin wrapper vs convenience flows; global vs per-command flags (e.g. `--config` and `-v`/`-q` global; `--json` only on commands that produce list/object).
- **Config:** Where config lives (one file with sections vs profile + overlay); layering (e.g. file + flags only for this phase).
- **Errors:** SDK error style (align with existing crates); CLI error presentation (single-line stderr by default vs multi-line with suggestion when `-v`).

</decisions>

<specifics>
## Specific Ideas

- Optional Hydra-style config for CLI (composition/overrides).
- No other specific references — open to standard approaches that fit the stack.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-sdk-cli-for-device-and-pipeline-config*
*Context gathered: 2026-02-20*
