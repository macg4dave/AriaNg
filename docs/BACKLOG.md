# Backlog

Items below are planned work, not authorization to execute everything at once.
Use IDs in task requests and update status with verified evidence as work lands.

| ID | Item | Status | Dependencies | Acceptance criteria |
| --- | --- | --- | --- | --- |
| TEST-01 | Debian/Nginx Docker test environment | Complete | Docker Linux containers | Image built; Compose service healthy; Nginx config valid; index, referenced assets, language file, license, and health endpoint served; missing/private paths return 404; runtime verified as unprivileged Debian |
| TEST-02 | Repeatable HTTP and browser smoke checks | Complete | TEST-01; Python; Playwright/Edge for UI | Clean build and tests passed; six routes, theme/language persistence, translation, mobile width, and uncaught browser errors checked with RPC blocked; live RPC remains separate work |
| TEST-03 | Disposable aria2 and automatic integration tests | Complete locally; CI configured | TEST-01; Docker; Python; browser tooling for UI | Real token auth rejection, HTTP/WebSocket RPC, task controls, verified download bytes/checksum, removal and failure reporting passed; AriaNg-submitted download shown completed; push/PR workflow configured but remote run unverified |
| BASE-01 | Inventory core behavior | Complete; BASELINE.md | None | Document affected task operations, RPC behavior, settings, multi-server isolation, and UI compatibility; identify meaningful regression checks |
| PERF-01 | Establish performance baseline | Complete for documented synthetic workloads | BASE-01 | Define reproducible synthetic workload, record baseline and tool commands, and distinguish provisional targets from agreed targets |
| DEPLOY-01 | Define deployment interface | Complete; Linux over SSH | Hosting requirements | Specify external config schema, target platform, transport, artifact contents, deploy/dry-run/rollback interface; include no private values |
| DEPLOY-02 | Implement deployment | Complete for static releases; disposable SSH test passed | DEPLOY-01 | Meet deployment acceptance checks in DEPLOYMENT.md against a disposable destination; document commands and verified rollback |
| ARCH-01 | Select first server component | Planned | Concrete use case; PERF-01 if performance-driven | Record component boundary, Go/Rust rationale, API and authentication needs, compatibility requirements, and deployment consequences |
| IMPL-01 | Implement selected component or optimization | Planned | Relevant architecture decision and baseline | Preserve affected behavior, pass meaningful checks, and demonstrate any claimed improvements with comparable measurements |
| LOAD-01 | Validate larger loads | Planned | PERF-01; relevant implementation | Report throughput, tail latency, errors, resource use, overload behavior, and recovery against agreed targets |

Completion is recorded against scoped evidence, not documentation creation.
Stages 1-3 are closed for the static application; production sizing and future
gateway service deployment remain separate stage 4/5 work.

## Initial optimization work (2026-11-11; historical progress)

- BASE-01 / PERF-01: partial. List, file metadata, and RPC connection paths inspected;
  repeatable synthetic processing and browser checks recorded in PERFORMANCE.md.
  Full behavior inventory and representative download-load baseline remain open.
- IMPL-01: initial frontend pass implemented and checked: reuse unchanged file
  derivation, retain GID rows, avoid bitmap count arrays, bound list/global polls,
  and handle WebSocket timeout/disconnect recovery. Production symptom unverified.
- ARCH-01: Rust gateway candidate documented in ARCHITECTURE.md; API/authentication,
  deployment and measured server implementation remain proposed.
- LOAD-01: still open. No server throughput or production capacity claim.

BASE-01/PERF-01 are now closed by the inventory and workload/coverage contract in
BASELINE.md, supported by the recorded results in PERFORMANCE.md. This supersedes
the initial partial status above; production/peer-load validation is not claimed.

## Planned optimization follow-ups (2026-11-11)

Items below remain planned unless progress is explicitly recorded. Sequence and code references are
in [Roadmap](ROADMAP.md#optimization-sequence-2026-11-11). Existing IDs above retain
their acceptance criteria; these entries split the proposed work into reviewable
increments rather than marking the broader baseline or architecture work complete.

| ID | Item | Dependencies | Acceptance criteria |
| --- | --- | --- | --- |
| PERF-02 | Diagnose periodic stalls/disconnects | Initial PERF-01 evidence | Reproduce or document inability to reproduce with 20-25 synthetic tasks; vary files/peers/transfers/delay; compare HTTP/WS timing, errors, bytes, browser responsiveness, and upstream resources without sensitive telemetry |
| RPC-01 | Bound detail refresh and improve failure recovery | PERF-02 | At most one detail/peer refresh cycle outstanding; verify timeout, malformed/empty response, disconnect, route/server change, resource cleanup and recovery; any read retry is bounded and mutations are never replayed automatically |
| UI-01 | Bound chart/history resources | Initial frontend baseline | Remove destroyed chart/popover handlers, compare chart update cost, bound history count with server-aware keys, and verify memory/handler recovery across repeated navigation/server switches |
| GATE-01 | Define Rust gateway contract | PERF-02; ARCH-01 | Review versioned API, auth/authz, credential/allowlist boundary, isolation, resource limits, freshness, direct-mode compatibility and profile migration; record architecture decision |
| GATE-02 | Implement Rust transport/shared polling | GATE-01 | Authenticated browser integration; authorized upstream/security scopes isolated; bounded polling, snapshots, concurrency and payloads; deadline/cancellation and overload/recovery tests; disclose baseline and measurements |
| GATE-03 | Derive compact task summaries | GATE-02 | Preserve list actions, copy links, naming/count semantics and translations; fetch detail data on demand; measure response bytes and browser/server processing against equivalent inventories |
| GATE-04 | Implement query/pagination and evaluate push | GATE-03 | Filter/sort before paging, stable ordering and consistent snapshots, selection/task-operation parity; any push/delta design bounds slow clients and supports full resync; report latency and network results |
| GATE-05 | Package and validate gateway adoption | GATE-04; DEPLOY-01 | Rust formatting/Clippy/tests/locked release build; compatible frontend/binary packaging, external config, health/readiness, shutdown and disposable rollback checks; measured load, limits and static/direct-mode compatibility |
| UI-02 | Avoid repeated list filtering/sorting | PERF-02 | Invalidate derived lists on relevant task/search/order/language changes; preserve dynamic ordering, search, selection and controls; compare digest/search costs |
| UI-03 | Bound rendered task/file/peer rows | PERF-02; coordinate with GATE-04 | Compare responsiveness and DOM size; preserve themes/mobile/translations, cross-page selection, keyboard actions and queue drag positions; document any intentional UX change |
| UI-04 | Optimize piece/peer-health views | PERF-02 | Benchmark representative pieces/peers; preserve piece accuracy and health semantics; compare CPU, allocation/memory and rendering cost for the chosen implementation |
| RPC-02 | Reduce redundant refresh traffic | PERF-02; RPC-01 | Measure coalescing/visibility/events where appropriate; verify freshness, notification behavior, reconciliation, bounded resume, isolation and request/byte reduction |
| MAINT-01 | Make verification effective and update dependencies | Existing build/regression checks | Real test/lint execution with inherited findings tracked; incremental locked dependency upgrades; standard and all-in-one build plus affected behavior checks; measure asset/startup changes before claiming benefits |
| TOOL-01 | Replace Python runners with Rust | Existing Docker/integration checks | Preserve fixture lifecycle, credential redaction, HTTP/RPC checks, failure exits and optional browser coverage; document Rust commands and retire replaced runners only after parity passes |

PERF-02 is in progress: `tests/performance/stall-browser.cjs` passed an eight-case
HTTP/WebSocket matrix with 20-25 tasks, 1/100 files per task and 0/1500 ms reply
delay. The UI received four fresh values per case with no overlapping list polls
or observed long tasks. This is an intercepted browser fixture, not real aria2
load; peer/transfer variation, upstream CPU/memory, transport failures and extended
soak runs remain open. See PERFORMANCE.md for measured values and limitations.

PERF-02 additional evidence: the live browser runner passed HTTP/WS with 20 queued
tasks and 25 tasks including four active rate-limited transfers. Each case
observed disconnection, recovery and subsequent detail refresh. No freeze was
reproduced. Peer load, prolonged runs and synchronized resource metrics remain open.

RPC-01 is partially implemented: one outstanding detail/peer cycle, settlement
fallback, destroyed-route guards, temporary-error recovery on normal polling ticks,
and malformed task/peer response checks. Deterministic checks and live HTTP/WS
detail navigation passed. Transport cancellation, explicit server-switch/BitTorrent
browser parity and retry-backoff evaluation remain open; mutations are not replayed.

UI-01 source work and focused resource verification are complete: reference/revision
chart updates, removed resize/popover resources, 32 retained task histories per
current RPC-session namespace and bounded samples. The chart resource check compares
100 retained histories/20 leaked handlers before with 32/zero after, verifies
popover destruction, sample limits, chart freshness and namespace reset. Extended
heap/real-peer soak validation remains stage 5 work; no heap-size claim is made.
