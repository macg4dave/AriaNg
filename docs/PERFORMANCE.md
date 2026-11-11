# Performance

## Rust gateway compatibility/shared-read stage (2026-11-11)

Source: working-tree `server/gateway/`, compared with direct HTTP delivery to the
same synthetic upstream. Windows x64, Rust 1.99.1, release optimization, four Tokio
workers. Command:

```sh
cargo test --release --locked --manifest-path server/gateway/Cargo.toml --test load -- --ignored --nocapture
```

One process hosts the fixture upstream, gateway and 20 HTTP clients. Each phase
issues five synchronized rounds at one-second spacing, 100 successful logical
reads of a fixed 25-task list with 100 files/task. JSON bodies are about 294 KiB;
there are no real downloads, peers, browser rendering or slow readers. Direct
delivery runs first; gateway delivery runs second in the same process. The gateway
polls one observed full-list signature, with one-second freshness. One recorded
run of the final shared-immutable-snapshot implementation:

| Metric | Direct | Gateway |
| --- | --- | --- |
| Completed requests / errors | 100 / 0 | 100 / 0 |
| Upstream requests during phase | 100 | 4 |
| End-to-end HTTP p95 / p99 ms | 18.281 / 20.205 | 13.164 / 14.378 |
| Logical requests/s over observed interval | 24.820 | 24.854 |
| Browser-side JSON response bytes | 29,359,300 | 29,359,300 |
| Combined test-process CPU ms | 281.250 | 109.375 |
| End-of-phase process working set bytes | 25,939,968 | 40,452,096 |

Five rounds start at t=0 through t=4, so finite-interval throughput includes that
boundary effect; it is not a saturation result or capacity target. Latency measures
request send through complete response receipt. CPU is OS-reported process CPU,
including client, upstream and gateway, sampled by the Rust harness using
PowerShell Get-Process on Windows (or procfs on Linux). Working set is an endpoint
sample, not peak/isolated gateway memory or reclaimed heap. Direct and gateway
phases share process/allocator/pools and are order-biased; CPU deltas can include
minor polling/sampling work at the boundaries. Upstream count can vary around
freshness/timer boundaries. These short, single-run results demonstrate shared
upstream reads, not production latency/CPU superiority. Full payload delivery
leaves downstream bytes unchanged and adds snapshot memory. An earlier deep-copy
implementation was slower and was replaced with shared immutable snapshot values.
No production improvement or reported-freeze fix is established.

Functional checks: 11 Rust tests passed, covering admission/slot/body/response
bounds, authentication/origin and scope isolation, shared reads, failure/recovery,
idle eviction, mutation invalidation, multicall validation and cancellation of slow
polls at shutdown. The explicit `--test live -- --ignored --nocapture` check passed
against disposable Compose aria2 with two browser contexts, waiting/detail/mobile
views, multicall, task option changes and unauthorized access. It bridges the
same-origin browser route to the actual gateway HTTP listener, substituting for a
production proxy. It removes its paused fixture task. No private production config
was read. Source startup, limits and remaining GATE-03/04/05 work are in GATEWAY.md.

Still unmeasured: isolated upstream/gateway CPU/RSS, long peer/transfer soaks,
slow-reader transport buffers, Linux runtime behavior, representative production
inventories/users, and browser responsiveness/network effects with the gateway.
No compact summaries or backend sorting/pagination are implemented in this stage.

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

### PERF-02: first stall diagnostic matrix (2026-11-11; partial)

Revision: frontend at `0697bea`, plus the new working-tree diagnostic. Windows
x64, Node 26.13.0, headless Edge 157.0.4341.46, existing local Docker static UI.
Run `node tests/performance/stall-browser.cjs` after starting the local web
service and installing the optional browser tooling described in DEVELOPMENT.md.
No application behavior changed in this increment; these are baseline observations.

One fresh browser context per case, one client, one-second list/global polling,
20 or 25 active synthetic task records, 1 or 100 files/task. RPC is intercepted
with successful JSON-RPC fixtures; there are no real transfers or peers. Initial
full data respects requested keys, followed by basic replies. Each reply advances
completed length, and an Angular watcher verifies four observed values per case.
Each case takes roughly 3-8 seconds; one matrix run is recorded below.

| Transport | Tasks | Files/task | Delay ms | Fixture reply p95 ms | Digest p95 ms | Maximum update gap ms | Response bytes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| HTTP | 20 | 1 | 0 | 15.4 | 2.0 | 1011.1 | 15826 |
| HTTP | 25 | 1 | 0 | 14.7 | 1.4 | 1010.7 | 19497 |
| HTTP | 25 | 100 | 0 | 13.7 | 1.8 | 1013.5 | 312636 |
| HTTP | 25 | 100 | 1500 | 1516.2 | 5.0 | 2017.3 | 312636 |
| WebSocket | 20 | 1 | 0 | 2.6 | 6.5 | 1002.3 | 15818 |
| WebSocket | 25 | 1 | 0 | 14.4 | 7.5 | 1014.2 | 19513 |
| WebSocket | 25 | 100 | 0 | 14.7 | 6.9 | 1000.3 | 312640 |
| WebSocket | 25 | 100 | 1500 | 1518.7 | 6.4 | 2016.6 | 312636 |

All cases observed four fresh list values, maximum one list request in flight,
zero uncaught browser errors and zero observed browser long tasks (50 ms or more).
Animation-frame gap p95 stayed about 16.8 ms; maximum gap was 33.3 ms. The slow
reply cases updated about every two seconds because timer ticks skip outstanding
requests. That is reduced freshness without an observed browser freeze here.

The reply metric measures fixture request receipt through serialization/send,
including injected delay, not end-to-end RPC or upstream latency. Bytes count
JSON response bodies across all completed RPC methods, excluding wire framing;
timer scheduling and generated IDs can cause small differences. Digest timing
uses an instrumented root digest after Angular startup; frame gaps approximate
scheduling responsiveness, not paint completion. Four replies provide very few
latency samples, so p95 is effectively their maximum. Instrumentation adds work.
Do not infer transport superiority or production capacity from these timings.

The reported periodic freeze/disconnection was not reproduced in this short
success-only fixture. PERF-02 remains open: real disposable aria2 workloads with
active transfers, file/peer variation, HTTP/WS failures/timeouts, upstream resource
measurements, longer observation and explicit connection-state diagnostics are
still required. No private configuration was read. Existing WebSocket recovery
checks also passed; no new production fix or performance improvement is claimed.

### PERF-02 live fixture and RPC-01 detail bounds (2026-11-11; partial)

Frontend baseline `0697bea`; working-tree detail changes compared with HEAD by
`node tests/performance/detail-refresh.cjs`. Same Windows/Node/Edge environment
as above; Debian Compose aria2 with its existing four-transfer concurrency limit.
Commands:

```sh
node tests/performance/detail-refresh.cjs
node tests/performance/task-processing.cjs
node tests/performance/websocket-recovery.cjs
npm run build
docker compose up --build -d --wait web
node tests/performance/live-stall-browser.cjs
```

The deterministic detail check produced 21 requests before versus one after
across 20 timer ticks while status was outstanding. The updated guard also holds
through the initial peer fetch. Checks passed for transient errors, rejected
transport/timeout, settlement without callback, authorization failure, file
editing, destroyed routes, malformed/empty/wrong-GID task results, invalid peer
results and a peer fault alongside valid task data. These use actual browser
controller/service code with simulated dependencies; they are not timing or
memory benchmarks. Temporary failures now resume on subsequent configured ticks
instead of permanently cancelling detail polling. Authorization still stops it.

The live runner uses one isolated browser client at a time, one-second polling,
20 paused single-file HTTP tasks, then 25 total tasks with four active transfers
limited to 1 KiB/s each and 21 paused. Each transfer uses the local 64 KiB fixture;
there are no BitTorrent peers. Four list replies are measured per case. It then
interrupts browser traffic (HTTP aborts or WebSocket close), observes Disconnected,
restores HTTP traffic or explicitly reconnects WebSocket, verifies list recovery,
and navigates directly to a paused task detail route for two combined refreshes.

| Transport | Total / active tasks | RPC p95 / p99 ms | Baseline response bytes |
| --- | --- | --- | --- |
| HTTP | 20 / 0 | 7.14 / 7.14 | 16402 |
| WebSocket | 20 / 0 | 4.09 / 4.09 | 16402 |
| HTTP | 25 / 4 | 3.90 / 3.90 | 17187 |
| WebSocket | 25 / 4 | 2.31 / 2.31 | 17186 |

All four cases passed disconnect/recovery and detail refresh with no uncaught
browser errors or observed long tasks. Frame-gap p95 was about 16.8 ms; maximum
was 50 ms. HTTP cases aborted two and one requests respectively; WebSocket cases
each closed a connection without injecting an RPC error. Response bytes count
JSON bodies for all measured methods, not wire framing. RPC latency is measured
at the Playwright interception boundary from request receipt to upstream reply;
it excludes browser decode/digest, and HTTP interception adds overhead. Samples
are few (p95/p99 effectively the maximum), cases last seconds, and these numbers
do not establish transport superiority or capacity. A pointer click did not finish
in a preliminary WebSocket run; direct route navigation is the checked workflow,
and modal/pointer interaction remains unverified.

The reported freeze was not reproduced. This only exercises small HTTP files and
the waiting-list/detail routes. Larger file inventories, real BitTorrent peers,
active-list rendering, prolonged runs, explicit server switches, slow/malformed
wire replies and synchronized upstream CPU/memory still need coverage. Docker
resource spot checks were unsynchronized and are not a load-resource baseline.
Task records were removed; partial files remain disposable. No production values
were used. Standard build and existing task/WebSocket regressions passed. Direct
ESLint comparison on normalized LF found no added findings in the two changed
source files (55 to 54 total); inherited lint issues remain.

### UI-01 chart/history resources (2026-11-11)

Baseline: chart and monitor sources at `0697bea`; updated: working-tree sources.
Command: `node tests/performance/chart-resources.cjs`. Windows x64, Node 26.13.0,
headless Edge 157.0.4341.46, served local UI and actual Angular/ECharts libraries.
The harness registers isolated Angular modules from both source versions, without
RPC access or private settings. No saved data or settings format changes.

Workload: 100 distinct synthetic task histories; a 300-sample chart after 500
writes; 20 compile/destroy cycles; one shown/destroyed popover; a synthetic RPC
namespace change; five repetitions of 500 quiet digests on one chart. Median
elapsed time for those digest batches is reported, not per-refresh latency.

| Measurement | Before | After |
| --- | --- | --- |
| Retained task histories after 100 distinct keys | 100 | 32 |
| Retained resize handlers after 20 chart destroy cycles | 20 | 0 |
| Popover resources removed on scope destruction | No | Yes |
| Histories/global stat isolated after namespace change | No | Yes |
| Median time for 500 quiet chart digests | 3.7 ms | 0.6 ms |

Fresh samples still update the actual chart; the 300-sample task capacity remains.
Charts now watch data reference/revision instead of deep comparisons. A monitor
write increments the revision; initial popover display also applies current data.
The chosen 32-history LRU bound is a volatile client retention policy, not an
agreed capacity target. Global history remains separate with its 120-sample limit.
The opaque namespace follows the page-lifetime RPC context; real profile switches
reload the page. No destinations or credentials enter history keys.

These are short isolated chart/resource checks, not whole-app heap, paint or
network measurements. The harness observes handler/history ownership and chart
disposal, not reclaimed heap bytes; extended memory/peer-load soak remains stage 5
work. Network/RPC frequency and bytes are unchanged by this change. No server
throughput or production freeze fix is claimed. Rust deployment validation is
functional tooling evidence in DEPLOYMENT.md, not an application performance result.

The standard and all-in-one builds also passed, along with HTTP smoke, the JS
six-route/theme/language/mobile smoke and existing detail/task/WebSocket checks.
Direct ESLint analysis of the four UI-01 source files found the same inherited
rule counts as HEAD after LF normalization; no new findings were introduced.
The Python browser smoke entry point could not run because its Playwright package
is absent; the equivalent JS smoke passed using the installed optional tooling.
