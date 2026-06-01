# Stream semantics

This document defines consumption model, timestamps, and "latest sample" semantics for NeuroHID stream types so SDK/CLI and runtime behavior are predictable. It satisfies COMP-04 (stream consumption, timestamp, and latest-sample semantics documented and consistent).

**Scope:** LSL is specified in full; Serial, BrainFlow, and Mock are summarized where they differ or align with LSL.

### Conventions

- **Timestamp units:** NeuroHID normalizes to microseconds for `Sample.device_timestamp` and `Sample.system_timestamp` where the source provides time (LSL seconds → μs; others as applicable).
- **"Latest sample":** Always defined per stream type and, for LSL, per implementation (continuous forward vs drain-then-last). Downstream consumers that need drain-then-last can implement it on the received stream.
- **Overflow/drops:** Documented per type; LSL uses liblsl buffer semantics (max_buflen in seconds); others as noted.

---

## Schema (overview)

```bnf
stream_semantics   := consumption_model timestamp_semantics latest_sample_semantics overflow_behavior
consumption_model  := "blocking" | "non-blocking" | "callback" ; per stream type
timestamp_semantics := "remote_capture" | "local_capture" ; units (seconds/micros)
latest_sample_semantics := "continuous_forward" | "drain_then_last" ; per implementation
overflow_behavior  := "buffer_seconds" "drop_or_block" ; per stream type
```

---

## LSL (Lab Streaming Layer)

### Consumption model

- **Blocking pull:** `pull_sample(timeout)` blocks until a sample is available or the timeout (seconds) elapses. Used in a loop to consume the stream in order.
- **Samples in order:** Samples are delivered in acquisition order. No reordering.
- **Non-blocking check:** `pull_sample(0.0)` returns immediately. If no sample is available, the returned timestamp is `0.0` (and the sample/chunk is empty or invalid). So "no sample" is indicated by `timestamp == 0.0` (or equivalent in the binding).
- **Reference:** [LSL Stream Inlet](https://labstreaminglayer.readthedocs.io/projects/liblsl/ref/inlet.html) — `pull_sample`, timeout semantics.

### Timestamps

- **LSL return value:** The timestamp returned by `pull_sample` is the **remote capture time** in **seconds** on the LSL clock (arbitrary epoch).
- **Mapping to local time:** Use the inlet’s `time_correction()` (and periodic re-sync) to map LSL time to local clock. NeuroHID uses this where needed for alignment.
- **NeuroHID `Sample`:**  
  - `device_timestamp`: LSL timestamp converted to **microseconds** (`timestamp_seconds * 1_000_000`).  
  - `system_timestamp`: local wall-clock time at receive (microseconds), set when the sample is pulled.

So: **device_timestamp** = capture time on the stream (micros); **system_timestamp** = when NeuroHID received it (micros).

### Latest sample

Two well-defined behaviors:

1. **Drain-then-last (alternative for “only latest”):** Call `pull_sample(0.0)` repeatedly until it returns no sample (timestamp 0.0). The **last** sample pulled before the drain is the "latest" sample. Use this when a consumer wants a single, most-recent sample per tick.
2. **Continuous pull (NeuroHID LSL device today):** The runtime uses **blocking** `pull_sample(0.2)` in a loop and **forwards every sample** to the pipeline. There is no drain step; "latest sample" for this implementation is simply the **most recently received sample** in the continuous stream (i.e. the last one forwarded in the current processing window). Consumers that need strict "drain-then-last" semantics can implement that on top of the stream or use a different consumer; the NeuroHID LSL device does not change its 0.2 s timeout or loop logic in this phase — it remains continuous pull, every sample forwarded.

### Overflow and drops

- **Buffer size:** LSL inlets use `max_buflen` in **seconds** (when nominal sampling rate is set). Buffer length in samples is derived from rate × max_buflen.
- **When consumer is slow:** If the consumer does not call `pull_sample` fast enough, the inlet buffer can fill. Whether the implementation drops oldest samples or blocks is implementation-defined in liblsl; document the actual behavior for the NeuroHID stack (e.g. default 360 s, and whether drops or blocking occur) when known.
- **NeuroHID:** The LSL device runs a dedicated pull loop with 0.2 s timeout, so it acts as a steady consumer; buffer choice and any drop/block behavior should be documented in release notes or config when they are fixed (e.g. if max_buflen is set per connection).

---

## Serial

- **Consumption model:** Device-driven; samples arrive as the serial device sends them (blocking read in a loop or equivalent). Order is preserved.
- **Timestamps:** Same idea as LSL where applicable: device/origin time if provided by the protocol, plus local receive time. NeuroHID sets `device_timestamp` and `system_timestamp` similarly when the protocol supplies a time.
- **Latest sample:** Same as LSL for this implementation: **continuous forward** — every sample is forwarded; "latest" is the most recently received sample. Drain-then-last can be applied by a downstream consumer if needed.
- **Overflow/drops:** Depends on serial buffer and read rate; document in device-specific notes if relevant.

---

## BrainFlow

- **Consumption model:** Typically callback or polling from the BrainFlow API; samples in order.
- **Timestamps:** Follow BrainFlow API (board/local time). NeuroHID maps to `device_timestamp` / `system_timestamp` in microseconds where the API provides timestamps.
- **Latest sample:** Same as LSL where applicable: **continuous forward** unless a specific mode implements drain-then-last; "latest" = most recently received sample in the continuous stream.
- **Overflow/drops:** Board- and API-dependent; document per board or refer to BrainFlow docs.

---

## Mock

- **Consumption model:** Synthetic stream; samples generated in order (e.g. timer or loop).
- **Timestamps:** `device_timestamp` and `system_timestamp` set to synthetic or local time in micros for consistency with other streams.
- **Latest sample:** Same as LSL: **continuous forward**; "latest" = most recently emitted sample.
- **Overflow/drops:** Typically none (in-memory); no buffer overflow in the same sense as LSL.

---

## Summary table

| Stream type | Consumption       | Timestamps (device) | Latest sample (NeuroHID) | Overflow/drops   |
|------------|-------------------|----------------------|---------------------------|------------------|
| LSL        | pull_sample loop  | Remote capture (μs)  | Continuous; last received| max_buflen; TBD  |
| Serial     | Read loop         | As protocol / local  | Continuous; last received| Serial buffer    |
| BrainFlow  | Callback/poll     | Per API              | Continuous; last received| Per board/API    |
| Mock       | Synthetic loop    | Synthetic/local μs   | Continuous; last received| N/A              |

---

## Relation to COMP-04

COMP-04 requires that stream consumption, timestamp, and "latest sample" semantics are documented and consistent. This document provides:

- A single place for consumption model (how samples are pulled or received), timestamp meaning (remote vs local, units), and latest-sample behavior (continuous forward vs drain-then-last) per stream type.
- Explicit statement of NeuroHID LSL behavior: continuous pull with 0.2 s timeout, every sample forwarded; "latest" = most recently received. Drain-then-last is described as the alternative for consumers who want only the latest sample per tick.
- Overflow and drop behavior (LSL max_buflen; others noted) so pipeline and SDK users can reason about backpressure and loss.

## References

- [LSL Stream Inlet (pull_sample, timestamps, max_buflen)](https://labstreaminglayer.readthedocs.io/projects/liblsl/ref/inlet.html)
- NeuroHID: `crates/neurohid-device/src/lsl/device.rs` — LSL pull loop and timestamp conversion; see module doc for link to this spec.

---

## Document status

- **Format version:** 1
- **Aligned with:** COMP-04 (stream consumption, timestamps, latest-sample semantics documented and consistent).
- **LSL implementation:** See `crates/neurohid-device/src/lsl/device.rs`; module doc links to this spec.
