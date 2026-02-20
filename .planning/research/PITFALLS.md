# Domain Pitfalls — Biosignal/EEG Developer Tooling

**Domain:** Biosignal/EEG developer tooling (Hub-as-IDE, standalone runtime, composable SDK/CLI/formats, extensibility)
**Researched:** 2026-02-20
**Confidence:** MEDIUM (mix of official docs, LSL FAQs, and ecosystem search; some findings single-source)

---

## Critical Pitfalls

### Pitfall 1: Treating “latest sample” as “drain then take one” (LSL / stream consumers)

**What goes wrong:**
Consumers assume one `pull_sample()` (or equivalent) returns the most recent sample. They get the oldest buffered sample instead; in real-time pipelines this yields stale decisions and apparent latency or wrong triggers.

**Why it happens:**
Stream APIs are FIFO. The “newest” sample is the last one in the buffer; callers must drain the buffer first, then use the last sample (or use a small `max_buflen` so old data is dropped).

**How to avoid:**
- For “most recent sample” use cases: repeatedly pull with timeout 0 until no more samples; use the last one. Or set a short buffer (e.g. 1 s) so overflow discards old data.
- Document this in SDK/CLI and in any “real-time” or “live” examples.
- In runtime/decoder path: centralize sample consumption behind an abstraction that guarantees “latest only” or “drain-then-last” semantics.

**Warning signs:**
- Docs or examples that say “get latest sample” but show a single pull.
- Decoder/action latency that grows over time until restart.
- Tests that only check “we got a sample” instead of “we got the most recent sample for this time window.”

**Phase to address:**
- **Composable SDK/CLI/formats** (clear stream-consumption contracts and examples).
- **Standalone runtime** (runtime’s device/signal path must implement correct drain/latest semantics).

---

### Pitfall 2: LSL clock vs wall clock confusion

**What goes wrong:**
Timestamps are treated as wall-clock or mixed with other systems (e.g. event logs, recorder UI). Offsets and “impossible” ordering appear; sync with external hardware or other streams breaks.

**Why it happens:**
`lsl_local_clock()` is an arbitrary monotonic origin (e.g. platform boot), not UNIX time. Naive conversion to wall clock is wrong; device timestamps from hardware are on yet another clock unless explicitly documented and handled.

**How to avoid:**
- Use LSL’s time only for relative timing and stream synchronization unless you implement manual sync (see LSL Time Synchronization docs).
- If exposing timestamps to users or logs, document clearly: “LSL time” vs “wall clock” and provide a single, documented way to convert when needed.
- Store any constant offset (e.g. hardware delay) in stream metadata so it can be reproduced offline.

**Warning signs:**
- Logs or UI showing “timestamp” without stating which clock.
- Calibration or events that “shift” between sessions or machines.
- Docs that say “timestamp” without specifying clock.

**Phase to address:**
- **Composable SDK/CLI/formats** (timestamp semantics in formats and SDK docs).
- **Hub-as-IDE** (visualization and event timelines must use consistent clock and labels).

---

### Pitfall 3: Hub and runtime sharing code paths that assume a GUI

**What goes wrong:**
Standalone runtime starts failing or behaving differently (e.g. blocking on UI, different config source, or missing “headless” branch). Users lose trust in “run in background” and fall back to keeping the Hub open.

**Why it happens:**
Convenience leads to calling GUI or Hub-specific code from shared orchestration; or config/state is read only from Hub-owned locations. The runtime is treated as a “Hub with the window closed” instead of a first-class, headless surface.

**How to avoid:**
- Strict boundary: runtime has its own entrypoint, config resolution, and task graph; it must not depend on egui/eframe or Hub screens.
- Shared logic lives in crates used by both (e.g. neurohid-core, neurohid-signal); Hub and service are separate binaries that both use that core.
- Validation: “neurohid-service only” and “Hub only” both pass the same contract tests (e.g. same decoder in the loop, same latency targets).

**Warning signs:**
- Runtime binary or tests that pull in `neurohid-hub` or GUI crates.
- “If the Hub is closed, do X” special cases in core.
- Different default config or profile resolution for Hub vs service.

**Phase to address:**
- **Standalone runtime experience** (design and enforce headless vs Hub boundary from the start).
- **Hub-as-IDE** (Hub consumes runtime via IPC/facade, does not replace it).

---

### Pitfall 4: Extensibility added via “one more enum” instead of contracts

**What goes wrong:**
New device or output type requires editing a central enum and every match/switch. Compilation and review load grow; third parties cannot add a new backend without forking.

**Why it happens:**
Early design uses enums for “device type” or “output type”; when “one more” is needed, the path of least resistance is another variant. Trait-based or plugin contracts are deferred.

**How to avoid:**
- Model “device” and “output” as traits (or equivalent contracts) from the start; new backends implement the trait and register, without extending a central enum of all known types.
- Document the contract (e.g. discovery, connect, stream lifecycle; or action emission, rate limits). SDK and docs describe how to add a new device or outlet.
- Keep a small, curated list of “blessed” backends in-tree; extensibility is “implement this trait and drop in,” not “send a PR to add an enum variant.”

**Warning signs:**
- `match device_type { Lsl | Mock | Serial | BrainFlow => … }` with no path for “other.”
- Docs that say “we support LSL, Mock, Serial” but no “how to add a new device.”
- Proliferation of feature flags per backend instead of one extension mechanism.

**Phase to address:**
- **Extensibility** (trait-based device/outlet contracts, registration, and docs).
- **Composable SDK/CLI/formats** (SDK exposes extension points and examples).

---

### Pitfall 5: Format or schema evolution with no version and no compatibility story

**What goes wrong:**
Saved profiles, calibration results, or stream metadata change layout or semantics; old files break or are silently misinterpreted. Users and scripts cannot rely on “one NeuroHID version” reading “another version’s” output.

**Why it happens:**
Formats are implied (e.g. “we write JSON”) without a schema or version field. When the code evolves, there is no way to detect or migrate old data.

**How to avoid:**
- Every persisted format has a version field (and preferably a schema or doc that defines it). Readers check version and either support a compatibility window or refuse with a clear error.
- Document compatibility: “Profiles written by 1.x are readable by 2.x; 0.x is not supported.”
- Prefer additive evolution (new optional fields, deprecated fields ignored) over breaking renames or type changes when possible.

**Warning signs:**
- Serialized config or profile with no `version` or `schema_version`.
- Changelog that changes “the” format without mentioning migration or compatibility.

**Phase to address:**
- **Composable SDK/CLI/formats** (versioned formats and compatibility policy).
- **Coherent standard path** (docs and defaults reference same versions and compatibility).

---

### Pitfall 6: Real-time pipeline latency treated as “average” instead of “worst-case / jitter”

**What goes wrong:**
BCI feels “sometimes fine, sometimes laggy” or triggers miss. Benchmarks report “mean latency 25 ms” but 99th percentile or jitter is high; OS scheduling, GC, or buffer buildup cause sporadic stalls.

**Why it happens:**
Optimization and validation focus on average latency. Real-time control is sensitive to tail latency and jitter; a single long stall can break a trial or a user’s trust.

**How to avoid:**
- Measure and document latency distribution (e.g. p50, p95, p99) and, where possible, jitter. Use timestamped probes (e.g. event at source → action at output).
- Design for bounded buffers and backpressure: avoid unbounded queues between device → signal → decoder → action; define what happens when the pipeline can’t keep up (drop oldest, pause, or fail fast with clear state).
- In validation harness, add latency/jitter tests (e.g. Soak, LatencyMatrix) and gate on percentile, not only mean.

**Warning signs:**
- Only “average latency” in docs or dashboards.
- Unbounded channels or buffers in the hot path.
- No test that asserts p95 or p99 latency under load.

**Phase to address:**
- **Standalone runtime experience** (runtime must meet latency/jitter targets without Hub).
- **Coherent standard path** (validation and docs set expectations for real-time behavior).

---

### Pitfall 7: Calibration or session state not tied to identity and not reproducible

**What goes wrong:**
Calibration is run but the resulting model or parameters are not clearly tied to “which profile / which session / which device.” Reloading “the same” setup yields different behavior; or state is lost on restart and users must recalibrate without understanding why.

**Why it happens:**
State is stored in ad-hoc paths or under keys that don’t include profile/session/device; or calibration metadata (e.g. channel layout, sample rate) is not stored with the model, so the runtime cannot verify compatibility.

**How to avoid:**
- Bind calibration artifacts to a stable identity: profile id, session id, device id/serial (or hash of config). Store enough metadata with the artifact to reproduce or validate (channel count, rate, format version).
- Document “this calibration is for profile X, device Y, date Z” in UI and in exported files.
- On load, runtime checks that the current device/config matches (or warn and offer to recalibrate).

**Warning signs:**
- Calibration result saved without profile or device identifier.
- “Load decoder” with no check that the current stream matches the decoder’s expectations.
- Users reporting “it worked yesterday” with no way to see what changed (profile/device/session).

**Phase to address:**
- **Hub-as-IDE** (calibration wizard and visualization show and persist identity + metadata).
- **Coherent standard path** (docs and defaults explain calibration → profile → runtime flow and reproducibility).

---

### Pitfall 8: IPC / process boundary treated as “same process” (Rust–Python)

**What goes wrong:**
Large payloads or high-frequency messages cause high latency, memory spikes, or serialization errors. Assumptions that “it’s local” lead to no timeouts, no backpressure, and no clear handling of “Python not running” or version mismatch.

**Why it happens:**
IPC is local (named pipe / loopback), so it’s easy to assume it’s cheap and always available. Copying big buffers across the boundary or blocking on the other side is not modeled; reconnection and versioning are afterthoughts.

**How to avoid:**
- Design IPC around clear message types and size expectations; avoid sending raw large arrays when a reference or chunked protocol can be used. Prefer small control + optional bulk.
- Always set timeouts and handle “no response” (reconnect, degrade gracefully, or fail with a clear error). Document “Python bridge optional” behavior.
- Version the protocol (or handshake) so Rust and Python can detect mismatch and report it instead of silent corruption.

**Warning signs:**
- Sending full signal buffers or big tensors over IPC in the hot path.
- No timeout on “wait for Python response” in runtime.
- No protocol or app version check on connect.

**Phase to address:**
- **Standalone runtime experience** (runtime must behave correctly when Python is absent or slow).
- **Composable SDK/CLI/formats** (IPC contract and compatibility are part of the “standard path”).

---

## Moderate Pitfalls

### Pitfall 9: Hub-as-IDE does “everything,” so it never feels like an IDE

**What goes wrong:**
The Hub becomes a dashboard of every feature (devices, calibration, visualization, Python lab, Jupyter, settings) with no clear “workbench” for the main workflow (setup device → calibrate → train → run). Power users can’t replicate a focused “coding + run” loop.

**How to avoid:**
Define a primary workflow (e.g. device → calibration → decoder training → run) and make that the default “path” in the Hub (wizard or workbench). Keep secondary screens (advanced settings, Python lab, Jupyter) one click away but not in the critical path. Prefer “one main flow + panels” over “many equal tabs.”

**Phase to address:** Hub-as-IDE.

---

### Pitfall 10: Plugin or extension namespace and lifecycle undefined

**What goes wrong:**
Multiple plugins (or external device/outlet implementations) conflict on names, or load order changes behavior. Updating the host breaks plugins with no contract to test against.

**How to avoid:**
Define a small plugin/extension contract: naming (e.g. prefix or namespace), discovery (how the host finds extensions), and lifecycle (load, enable/disable, unload). Document “supported host version” and test one representative plugin in CI. Prefer “one plugin type, one contract” over multiple ad-hoc extension mechanisms.

**Phase to address:** Extensibility; Composable SDK/CLI/formats.

---

### Pitfall 11: Multiple streams or backends with ambiguous identity

**What goes wrong:**
User has two LSL streams or two devices; the app picks “one” (e.g. first resolved or last created) and the user doesn’t know which, or can’t choose. Scripts and automation break when another stream appears on the network.

**How to avoid:**
Resolution must be explicit: by name, hostname, serial, or type+serial. SDK and CLI should require or allow a strict filter (e.g. `name='Cognionics Quick-20' and hostname='My-PC001'`). When multiple matches exist, warn and document how the chosen stream is selected (e.g. last-created); prefer failing or prompting over silent arbitrary choice.

**Phase to address:** Composable SDK/CLI/formats; Hub-as-IDE (device list and selection).

---

## Minor Pitfalls

### Pitfall 12: LSL chunk size and push pattern wrong for rate/bandwidth

**What goes wrong:**
At high sampling rates, pushing single samples causes high OS/network overhead; at low rates, huge chunks add latency. Defaults are never tuned.

**How to avoid:**
Follow LSL FAQ: for small samples at high rate, use chunks (or chunk size on outlet); for large data, match stream type to avoid extra casting. Document recommended chunk size range (e.g. 5–30 ms of data) and make it overridable in device backends.

**Phase to address:** Standalone runtime; Composable SDK/CLI/formats (device backend docs).

---

### Pitfall 13: Bad channel / artifact handling deferred to “later”

**What goes wrong:**
Pipeline assumes all channels are valid; bad or flat channels corrupt covariance, spatial filters, or features. Decoder quality degrades and debugging is hard because “bad channel handling” was deferred.

**How to avoid:**
Identify and mark bad channels early (e.g. in signal task or device layer). Pass channel quality or mask through the pipeline; document how decoders/calibration handle bad channels (exclude, interpolate, or fail). Prefer “bad channel detection in the standard path” over “user does it in Python later.”

**Phase to address:** Coherent standard path; Hub-as-IDE (visualization and calibration show channel status).

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Single “device type” enum | Fast to add one more backend | Every new type touches core; no third-party extension | Never for public extensibility; only internal MVP with a clear “replace with trait” plan |
| No format version in saved state | Less code to write | Breaking changes force silent breakage or one-off migrators | Never; add version from first persisted format |
| “Latest sample” = one pull | Simple example code | Wrong semantics in production; hard to debug | Never; document drain-then-last or provide a helper |
| Hub and runtime share one binary with “headless” flag | One less binary to maintain | Coupling and accidental GUI deps in runtime | Never; keep runtime binary and dependency tree strictly separate |
| IPC without timeout | Fewer branches in code | Hangs when Python crashes or is slow | Never; always timeout and handle disconnect |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| LSL | Using one pull for “latest sample” | Drain buffer (pull until empty), then use last sample; or set short max_buflen |
| LSL | Treating LSL timestamps as wall clock | Use LSL time for relative/sync only; document and centralize any wall-clock conversion |
| LSL | Sending raw structs across platforms | Use a single numeric format (e.g. cft_double64) or explicitly defined serialization; avoid compiler-dependent layout |
| Rust–Python IPC | Sending large buffers every frame | Chunk or reference; keep hot path small; version protocol |
| Multiple LSL streams | Resolving by type only (`type='EEG'`) | Resolve by name + hostname or serial when multiple streams possible; warn on ambiguity |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Unbounded buffer between device and decoder | Latency grows over time; memory grows | Bounded channels; drop-oldest or backpressure policy | Long runs; many channels or high rate |
| Decoder assumes “average” latency | Occasional missed triggers; “sometimes laggy” | Design for p95/p99; measure jitter; validate with percentile tests | Under load or on slower machines |
| Large IPC payloads in hot path | High CPU or memory at high rate | Small control messages; chunked or reference-based bulk | When trainer or visualization streams large data |
| Pulling single LSL samples at high rate | High CPU, unnecessary wake-ups | Use pull_chunk or outlet chunk size; match LSL FAQ for high rate | 1 kHz+ streams, many channels |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Storing credentials or API keys in config without keyring | Theft or leak of tokens | Use neurohid-storage + OS keyring for secrets; document “no secrets in plain config” |
| Loading untrusted plugins or device drivers without sandbox | Malicious code in process | Document “load only trusted code”; optional sandbox for plugin processes if needed later |
| Exposing IPC to network interfaces | Other machines controlling runtime or reading data | Bind IPC to loopback only; document that IPC is local-only |

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| “Runtime is running” with no visible feedback | User doesn’t know if decoder is active or which profile is loaded | Tray icon, status in Hub, or lightweight “runtime status” endpoint/cli |
| Calibration result with no “for which profile/device” | Confusion when switching devices or profiles | Always show and store “calibrated for profile X, device Y” |
| SDK docs show only happy path | Integration fails on disconnect, no Python, or wrong version | Document error handling, timeouts, and version compatibility in first-use examples |

---

## “Looks Done But Isn’t” Checklist

- [ ] **Stream consumer:** “Latest sample” is implemented as drain-then-last (or equivalent); tests assert recency, not just “got a sample.”
- [ ] **Timestamps:** All user-facing or logged timestamps document which clock (LSL vs wall); conversion is centralized and documented.
- [ ] **Runtime:** `neurohid-service` (or equivalent) has zero dependency on Hub/GUI crates; same contract tests pass for Hub-driven and service-only runs.
- [ ] **Extensibility:** At least one device and one output type are added via trait/plugin contract, not by editing a central enum.
- [ ] **Formats:** Every persisted format has a version field and a stated compatibility policy.
- [ ] **Latency:** Validation reports p50/p95/p99 (or similar); one test gates on tail latency or jitter.
- [ ] **Calibration:** Stored artifacts include profile id, device/session identity, and enough metadata to validate on load.
- [ ] **IPC:** Timeouts and “Python unavailable” are handled; protocol or app version is checked on connect.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| LSL “latest sample” wrong | LOW | Add drain-then-last (or small buffer) in one central consumer; fix docs and examples; add test. |
| LSL clock misuse | MEDIUM | Introduce a single “time namespace” and conversion layer; migrate logs/UI to use it; document. |
| Hub/runtime coupling | HIGH | Extract shared logic into core; remove GUI deps from service binary; add headless tests; regression test both surfaces. |
| Enum-based “extensibility” | HIGH | Introduce device/outlet traits; migrate one backend to trait; document; deprecate enum extension; add plugin example. |
| Format without version | MEDIUM | Add version to current format; add reader branch for “legacy” if needed; document compatibility. |
| Latency only measured as mean | LOW | Add percentile metrics and tests; set targets for p95/p99; fix unbounded buffers if present. |
| Calibration state unbound | MEDIUM | Add identity to existing artifacts (migration script if needed); validate on load; document. |
| IPC no timeout/version | MEDIUM | Add timeouts and disconnect handling; add version handshake; document and release. |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Latest-sample semantics (LSL) | Composable SDK/CLI/formats; Standalone runtime | Contract test: “latest sample” matches source; example in docs uses drain-then-last |
| LSL clock vs wall clock | Composable SDK/CLI/formats; Hub-as-IDE | Docs and UI label clock; no naive wall-clock use of LSL time |
| Hub vs runtime coupling | Standalone runtime; Hub-as-IDE | Service binary has no GUI deps; same contract tests for Hub and service |
| Enum instead of extensibility | Extensibility; Composable SDK/CLI/formats | New device/outlet added without changing central enum; extension doc and example |
| Format versioning | Composable SDK/CLI/formats; Coherent standard path | All persisted formats have version; compatibility statement in docs |
| Latency/jitter ignored | Standalone runtime; Coherent standard path | LatencyMatrix or Soak reports percentiles; test gates on p95/p99 |
| Calibration identity/reproducibility | Hub-as-IDE; Coherent standard path | Calibration stores profile+device+metadata; load path validates |
| IPC timeout/version | Standalone runtime; Composable SDK/CLI/formats | Timeout and disconnect tests; version check on connect |
| Hub does “everything” | Hub-as-IDE | Single primary workflow documented and default; secondary screens one click away |
| Plugin namespace/lifecycle | Extensibility | One extension contract documented; one plugin in CI |
| Ambiguous stream identity | Composable SDK/CLI/formats; Hub-as-IDE | Resolution by name+host/serial; warn or fail on multiple matches |
| LSL chunk/push pattern | Standalone runtime; SDK docs | Device backend docs and defaults follow LSL FAQ; overridable |
| Bad channel handling deferred | Coherent standard path; Hub-as-IDE | Bad channel detection in pipeline; docs and UI show channel status |

---

## Sources

- Lab Streaming Layer — FAQs (Get the newest sample, lsl_local_clock(), Latency, Multiple data types, Timestamp accuracy, Using device timestamps, High sampling rates, Chunk sizes, Multiple streams). <https://labstreaminglayer.readthedocs.io/info/faqs.html>
- Brain Products — Potential pitfalls when using LSL and how to avoid them. <https://pressrelease.brainproducts.com/lsl-pitfalls/>
- MOABB / EEG BCI reproducibility (hyperparameter scope, bad channel handling, train-test splits). ArXiv 2404.15319; MNE cookbook (bad channels, preprocessing).
- Open Ephys GUI — Plugin types, Creating a new plugin; PsychoPy plugin dev (session persistence, namespace). open-ephys.github.io; psychopy.org/developers/pluginDevGuide.html
- FieldTrip buffer, OpenBCI/Open Ephys latency docs (ring buffer, block duration vs jitter, closed-loop latency).
- Rust–Python IPC (serialization overhead, timeouts, zero-copy where applicable). Blog/discussion search results.
- BCI calibration/session (inter-session transfer, distribution shift). PMC / Frontiers articles on calibration and long-term use.
- SDK/API design (cognitive load, abstraction leaks, “time to hello world”). Compiler.today; Freeplay.ai; BCI API adoption (DIS 2018).

---
*Pitfalls research for: NeuroHID — biosignal/EEG developer tooling (Hub-as-IDE, standalone runtime, SDK/CLI/formats, extensibility)*  
*Researched: 2026-02-20*
