# Roadmap

Goal: customize this fork for the owner's servers, improve speed and behavior
under larger loads, and provide easy deployment without storing private details.
This roadmap describes direction; individual tasks are tracked in [Backlog](BACKLOG.md).

| Stage | Intended outcome | Current state |
| --- | --- | --- |
| 1. Project guidance | Shared agent rules, reusable prompts, and project docs | Documentation established; implementation work remains separate |
| 2. Baseline | Core behavior inventory and reproducible synthetic performance workload | Initial frontend/RPC baseline recorded; full workload baseline remains open |
| 3. Deployment | One-command deploy, dry-run, health checks, and rollback using external config | Local Debian/Nginx test container added; remote deployment is design only |
| 4. Targeted implementation | Optimize measured bottlenecks; introduce Go/Rust components where justified | Initial frontend refresh/RPC fixes implemented; Rust polling gateway assessed and remains proposed |
| 5. Load validation | Demonstrate improvements and recovery under agreed workloads | Awaiting targets and implementation |

Baseline and deployment preparation may proceed independently where inputs are
known. Server implementation requires a concrete use case and an architecture
decision. Do not assume that both a Go and a Rust implementation are required.

Open planning inputs include expected concurrency, task counts, update frequency,
latency/resource budgets, hosting capabilities, and branding/customization scope.
Record generic requirements here; keep actual infrastructure details external.

## Optimization sequence (2026-11-11)

The first frontend refresh/RPC changes and their limited synthetic measurements
are recorded in [Performance](PERFORMANCE.md). The work below is planned; adding it
to this roadmap does not implement it or authorize every backlog item. Prioritize
the reported periodic refresh freezes and disconnected status at roughly 20 or
more tasks before treating large task counts as the cause.

### Next: diagnose connection stalls and finish client resource bounds

1. Reproduce the symptom with disposable aria2 and synthetic workloads starting at
   20-25 tasks, then vary files per task, peers, active transfers, response sizes,
   and upstream delay. Compare HTTP and WebSocket. Measure RPC duration/errors,
   response bytes, browser long tasks, digest/paint timing, and upstream CPU/memory.
   Distinguish transport failure, stale data, and browser stalls in diagnostics;
   do not label all three as an unavailable aria2 service. Keep logs free of tokens,
   destinations, file paths, and raw request/response bodies. See PERF-02.
2. Extend outstanding-request guards and cancellation/lifecycle checks to task
   detail and peer refreshes. Inspect malformed/empty RPC responses, failures,
   route/server changes, and recovery. Evaluate bounded retry backoff with jitter
   for read polls; never automatically replay task mutations. See RPC-01.
3. Clean up chart resize handlers and popover resources on destruction, then
   measure explicit chart updates in place of deep watches. Bound retained task
   histories as well as samples per history, with server-aware keys and eviction.
   Verify memory recovery after repeated navigation and server switches. See UI-01.

### Rust gateway: incremental replacement of browser data work

Use one Rust application service for this component. Go remains an approved
language for a separately justified component; there is no planned parallel Go
implementation. Rust is chosen for the requested server direction and control of
snapshot memory/processing, not as evidence of a speed improvement. Existing aria2
continues managing downloads and the browser continues rendering the interface.

| Milestone | Existing responsibility to replace | Reviewable outcome |
| --- | --- | --- |
| GATE-01: contract and isolation | Direct browser connection configuration and transport boundary | Define versioned API, external upstream allowlist/credentials, browser authentication, per-user/server authorization, limits, and opt-in gateway profiles; preserve direct HTTP/WebSocket mode |
| GATE-02: transport and shared polling | RPC transport services plus list/global-status polling in `controllers/list.js` and `controllers/main.js` | One bounded polling loop per authorized upstream/security context, shared snapshots for clients with the same access, explicit freshness/status, deadlines, cancellation, and bounded overload/recovery behavior |
| GATE-03: compact summaries | List-related name/file-count derivation in `services/aria2TaskService.js` | List responses contain required summary fields; fetch file trees, pieces, and peers on demand for details; preserve retry/copy-link/selection operations and localized display semantics |
| GATE-04: query and updates | Search in `core/root.js`, sorting in `filters/taskOrderBy.js`, and full-list delivery | Filter and sort before pagination, stable ordering/snapshot semantics, selection across pages, and measured payload reduction; assess push/delta updates with bounded slow-client handling and full resynchronization |
| GATE-05: packaging and validation | Static-only deployment assumption for gateway users | Package compatible Rust release binary/frontend versions; external private configuration, health/readiness, graceful shutdown, rollback, Rust checks, and comparative load/resource results |

GATE-01 depends on PERF-02 and ARCH-01; each later gateway milestone depends on
the preceding one. GATE-02 must establish authentication and isolation before any
browser integration. Never share snapshots across different upstream credentials
or authorization scopes merely because the destination is the same. Deployment
preparation can proceed alongside implementation, but runtime adoption requires
GATE-05 and the contract in [Deployment](DEPLOYMENT.md). The gateway remains
proposed; its architectural boundary is in [Architecture](ARCHITECTURE.md).

### Along the way: reduce browser work

- UI-02: compute filtered/sorted lists when their data or search/order inputs change
  instead of repeating the work in every digest. Evaluate search debounce for large
  lists while retaining immediate selection and keyboard behavior. Dynamic sort
  fields must still update on fresh snapshots; do not cache by array identity alone.
- UI-03: introduce pagination or virtual scrolling where measurements justify it,
  including file and peer tables. Keep rendered DOM bounded and preserve responsive
  layouts, themes, translations, selection, keyboard actions, and waiting-list drag
  positions. Backend pagination and bounded rendering are complementary changes;
  client windowing can be evaluated before the gateway.
- UI-04: profile piece maps and peer-health calculation. `directives/pieceMap.js`
  creates an element per displayed piece, and peer-health processing expands peer
  bitmaps and repeatedly traverses availability counts. Evaluate canvas/grouped
  rendering and allocation/repeated-work reductions while preserving health values,
  piece accuracy, and detail behavior. Move computation to Rust only if measurements
  justify transporting/maintaining that data there.
- RPC-02: evaluate coalescing related read-only RPC requests, notification-triggered
  refresh with periodic reconciliation, and slower polling for hidden tabs or
  unobserved detail panels. Maintain explicit freshness, bounded resume refresh,
  notification behavior, and server isolation. Measure request counts and bytes;
  avoid substituting an unbounded event queue for polling.

### Later: maintenance and architecture decisions

- MAINT-01: wire meaningful regression checks into the normal test command and
  repair the Gulp lint task, with an explicit baseline for inherited findings.
  Audit and upgrade old dependencies incrementally, verify both build formats, and
  measure startup bytes/time before considering chart or route lazy loading.
- TOOL-01: migrate Python development/integration runners to Rust incrementally,
  preserving existing checks and keeping browser automation in suitable browser
  tooling. This improves maintenance consistency, not download/display performance.
- Consider a frontend framework migration or Web Workers/WASM only after targeted
  fixes leave a measured bottleneck. A rewrite needs affected-behavior baselines,
  settings migration if necessary, and a concrete benefit. Do not add services,
  persistent databases, or language boundaries without a demonstrated requirement.

Each implementation must record comparable before/after results and limitations
in PERFORMANCE.md. No production workload, capacity target, or improvement from
the proposed gateway has been established.
