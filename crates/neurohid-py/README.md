# neurohid-py

PyO3 Python bindings for the NeuroHID runtime — embeds the Rust service
in-process so Python ML code can access EEG data without socket transport or
JSON serialization overhead.

## Build

The crate produces a `cdylib` named `neurohid` (the Python module name).
It is built as part of the workspace and loaded by `neurohid_ml` at runtime:

```bash
cargo build -p neurohid-py
```

## Public API

### Lifecycle

| Class | Purpose |
|---|---|
| `RuntimeBuilder` | Configure and start the embedded runtime |
| `RuntimeHandle` | Commands, subscriptions, and trainer bridge |

### Typed Wrappers

| Class | Key Attributes |
|---|---|
| `Sample` | `values` (numpy), `quality` (numpy), `channel_count`, `timestamp_us` |
| `FeatureVector` | `values` (numpy), `timestamp_us` |
| `Action` | `action_type`, `confidence`, `timestamp_us` |
| `DecisionEvent` | `feature_values` (numpy), `decoder_confidence`, `signal_quality`, `action` |
| `ErrpWindow` | `channel_data` (2-D numpy, channels×samples), `channel_labels`, `sample_rate_hz` |
| `RuntimeEvent` | `sample()`, `feature()`, `action()`, `decision_event()`, `errp_window()` extractors |

Numeric getters return **numpy arrays** (single memcpy into a contiguous
buffer). Backward-compatible `values_list()` / `quality_list()` methods
return plain Python lists.

### Async Stream Iterators

`RuntimeHandle` exposes `subscribe_samples()`, `subscribe_features()`,
`subscribe_actions()`, `subscribe_markers()`, and `subscribe_events()` —
each returns a Python async iterator wrapping a tokio broadcast channel.

### Batch Methods

- `recv_sample_batch(n)` — collect *n* samples → 2-D numpy `(n, channels)`
- `recv_feature_batch(n)` — collect *n* feature vectors → 2-D numpy `(n, features)`

### Trainer Bridge

- `trainer_connect(session_id)` / `trainer_disconnect()`
- `trainer_send(envelope_dict)` — send a protocol v3 envelope
- `trainer_recv()` — receive as JSON string (legacy)
- `trainer_recv_typed()` — receive with typed payload objects (`DecisionEvent`, `ErrpWindow`, or plain dict)

## Dependencies

| Crate | Role |
|---|---|
| `pyo3` 0.28 | Python↔Rust bindings (abi3, free-threaded Python 3.14+) |
| `numpy` 0.28 | PyO3 numpy array interop |
| `pythonize` 0.28 | serde → Python object conversion |
| `pyo3-async-runtimes` 0.28 | tokio ↔ Python asyncio bridge |
