# NeuroHID Lab Kernel Protocol (v1)

The Hub Python Lab is now decoupled from a specific Python implementation.
It launches a configurable kernel command and communicates over newline-delimited
JSON on `stdin`/`stdout`.

Default command:

```text
uv run --directory python neurohid-ml lab-kernel --stdio
```

Configure this in Hub Settings:

- `UI > Lab kernel cmd`

## Transport

- Request: one JSON object per line written to kernel `stdin`.
- Response: one JSON object per line read from kernel `stdout`.
- Kernel diagnostics/logging may be written to `stderr`.

## Requests

### Execute

```json
{"type":"execute","request_id":1,"code":"print('hello')"}
```

### Reset

```json
{"type":"reset","request_id":2}
```

### Ping

```json
{"type":"ping","request_id":3}
```

### Shutdown

```json
{"type":"shutdown"}
```

## Responses

### Ready

```json
{"type":"ready","protocol":"neurohid_lab_kernel_v1"}
```

### Execute Result

```json
{
  "type":"execute_result",
  "request_id":1,
  "status":"ok",
  "stdout":"hello\n",
  "stderr":"",
  "result":null,
  "error":null,
  "exec_count":1,
  "duration_ms":4
}
```

### Reset Result

```json
{"type":"reset_result","request_id":2}
```

### Pong

```json
{"type":"pong","request_id":3}
```

### Error

```json
{"type":"error","request_id":1,"message":"..."}
```

## Notes

- The default Python kernel keeps execution state across cells (notebook-like).
- Any external runtime (Jupyter/marimo adapter, custom trainer runtime, etc.)
  can be plugged in if it implements this protocol.
- This protocol is intentionally separate from service runtime IPC
  (`neurohid-ipc`), so experimentation tooling can evolve independently while
  still interfacing with NeuroHID through existing service interfaces.
