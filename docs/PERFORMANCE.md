# Performance

## Status

An initial frontend processing baseline and connection-recovery regression checks
are available. No numeric capacity target or production load result is established.
The reported symptom is periodic refresh freezes and disconnected status at roughly
20 or more tasks; task count alone does not establish the cause.

## Measurement contract

Before optimizing, define synthetic or sanitized workloads covering relevant
concurrent users, RPC connections, task counts, update frequency, payload sizes,
and slow clients. Agree latency, throughput, error, and resource budgets where
possible; label provisional assumptions. Never store real server inventories.

Compare before and after using equivalent workload and environment conditions.
For servers, capture throughput, p95/p99 latency, errors, CPU, and memory where
relevant. For the frontend, measure responsiveness, large-list rendering, and
network/RPC use. Use release builds for compiled-server performance measurements.

Exercise disconnects, upstream failures, saturation, and recovery. Check bounded
concurrency, queues, payloads, retries, and memory growth. Caching and batching
must preserve freshness, correctness, and isolation. Profile before adding pooling,
complex concurrency, or low-level optimizations.

## Results record

### Initial refresh and RPC work (2026-11-11)

Baseline: `4d0ee89`; comparison: working-tree changes based on that revision.
Environment: Windows x64, Node 26.13.0; headless Edge 157.0.4341.46 for UI checks.
Fixtures contain synthetic file metadata only. Commands:

```sh
node tests/performance/task-processing.cjs 4d0ee89
node tests/performance/websocket-recovery.cjs
npm run build
python tests/run_docker.py
npm install --prefix .local/browser-tools --no-package-lock --no-audit --no-fund playwright
node tests/performance/list-browser.cjs
node tests/performance/rpc-browser.cjs
```

The processing harness loads both versions of the actual task service, warms each
for five iterations, then measures 30 cached/basic refreshes in one process. It
uses one client, one list, no actual RPC, and unchanged cached file metadata.
The intended update frequency is the default one second; timings below measure
CPU work per refresh without sleeping. List payloads exclude piece fields, as
the actual list RPC does. Network bytes are unchanged by metadata reuse.

| Tasks | Files/task | Baseline median / p95 ms | Updated median / p95 ms |
| --- | --- | --- | --- |
| 1,000 | 1 | 1.478 / 2.220 | 0.782 / 0.995 |
| 1,000 | 100 | 30.877 / 40.139 | 0.862 / 0.982 |

These are processing microbenchmarks, not end-to-end responsiveness or server
capacity measurements. The cached files are already stale between full responses
in the original protocol; skipping their repeated derivation does not change RPC
freshness. Full responses and detail views still process fresh file data. Completed
piece counting now avoids temporary per-piece arrays in task/peer processing;
zero pieces produce zero completed pieces even if a malformed bitmap is supplied.

The built UI retained DOM rows across ten full object replacements for each of
25 and 250 tasks. Updated digest medians were 1.1 and 5.2 ms (maxima 2.0 and
6.0 ms). Search, row counts, mobile width, and absence of uncaught browser errors
passed. This is an updated-build check without a browser timing baseline; it does
not measure paint, scrolling, themes, or an actual download load.

Deterministic slow-RPC tests show that 20 timer ticks during an outstanding list
request issue one request instead of 21. Failure releases the guard; authorization
failure stops polling and late route responses are ignored. Global status polling
uses the same single-request guard. WebSocket tests cover the existing 20-second
HTTP deadline applied to WebSocket responses, close without automatic reconnect,
explicit reconnect, send failure, late replies, and subsequent success. Requests
while a socket is connecting/closed fail immediately instead of being queued for
later replay. These are intentional connection behavior changes; they do not make
an overloaded or unavailable aria2 instance healthy.

Remaining measurements: the reported 20-task failure with representative file
counts, HTTP versus WebSocket latency/errors and payload sizes, long tasks in the
browser, actual paint responsiveness, and upstream CPU/memory during download load.
No live user configuration or remote server was inspected. The existing Docker
HTTP and real aria2 integration suite passed against disposable fixtures.

When measurements exist, record the task ID, revision, exact commands, sanitized
environment description, workload, duration/repetitions, before/after results,
and limitations here or in linked reports. Do not infer capacity from language
choice or present unmeasured improvements as facts.
