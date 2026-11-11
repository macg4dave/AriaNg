# Architecture

## Current system

This fork starts from AriaNg: an AngularJS browser application built with Gulp.
The standard build produces static assets in `dist/`; the all-in-one build embeds
assets in a single HTML file. An optional Rust RPC gateway now lives in
`server/gateway/`.

The browser communicates with separately managed aria2 services using HTTP or
WebSocket RPC. The UI supports multiple RPC servers, task management, settings,
translations, responsive layouts, and light/dark themes. Browser-local settings
and settings exports can contain private connection information.

Node.js and npm are build tools, not production application server requirements
for the current static deployment. Web hosting, aria2, authentication, and any
reverse proxy are separate infrastructure. See [Development](DEVELOPMENT.md) for
the source map and commands.

## Agreed direction

Local testing uses a multi-stage Docker build: Node 24/Bookworm builds static
assets, and Debian Bookworm with packaged Nginx serves them as an unprivileged
user. Compose adds a separate Debian aria2 service, using an ignored random local
RPC token and disposable download storage. The web service forwards same-origin
RPC traffic to aria2. Only the web port is published on host loopback; the internal
Docker network restricts tests to a locally served fixture. This test arrangement
does not select production hosting or authentication; see [Docker testing](DOCKER.md).

- New or rewritten application server components use Go or Rust according to
  workload and maintainability. The optional polling gateway uses Rust as requested,
  with Axum/Tokio and reqwest; aria2 remains the download engine.
- Static deployment now uses the dependency-free Rust operator in `tools/deploy/`,
  with a small remote POSIX-shell helper. Rust follows the tooling policy and
  keeps configuration/validation/deadlines in one portable binary; this is not
  an application backend or a performance claim. Linux/SSH release roots, strict
  host verification, artifact allowlisting, atomic activation and checked rollback
  are the supported contract, validated against disposable Debian SSH/Nginx.
- Rewrites are welcome if affected core functionality is preserved and settings
  or user data have a migration path when needed. Preserve upstream attribution.
- Keep service boundaries simple. Add a backend for a concrete requirement;
  existing aria2 and hosting do not require replacement merely for language choice.
- Private infrastructure configuration remains external. Public browser assets
  cannot conceal credentials or destination addresses from browser users.
- The gateway fixes destinations/credentials in external configuration, requires
  a unique bearer credential per full-control scope, and isolates scope snapshots.
  Existing HTTP profiles opt in through an operator-configured same-origin proxy.
  This service is not packaged by the static deployment operator.

## Decisions still open

### Rust backend assessment (2026-11-11; incrementally implemented)

The Rust backend supplies compatible RPC, bounded shared polling and forwarded
detail/mutation calls. `gateway.listTasks` now derives compact summaries and filters,
sorts and pages them on the server. Queries share one canonical snapshot per list
source, with revision checks that reject changed snapshots between page requests.
Summaries omit files/pieces/peers; detail RPC retains full data. Each source is
bounded to 1,000 tasks (oversized sources fail rather than silently truncate), and
pages to 200 rows. Explicit browser gateway profiles now request 100-row compact
pages with server search/order, revision recovery and page-scoped selection.
Direct HTTP/WebSocket profiles retain compatible aria2 methods. See
[Gateway integration](GATEWAY-INTEGRATION.md) for configuration and behavior.

Current code findings and initial fixes:

- `controllers/list.js` and `controllers/main.js` polled without waiting for the
  preceding list/global-stat reply. Each now permits one outstanding request.
- `services/aria2WebSocketRpcService.js` had no per-response deadline and could
  retain requests after close. Requests now fail on close or after 20 seconds,
  and disconnected operations are rejected instead of queued for replay.
- `services/aria2TaskService.js` revisited cached file arrays on every basic list
  response. Those responses contain no files, so file derivation is now reused.
  Full responses and detail views retain full derivation. Task/peer completed-piece
  counting no longer expands bitmaps into temporary boolean arrays.
- `views/list.html` now identifies rows by GID within the selected server's list,
  allowing Angular to retain DOM rows when full snapshots replace task objects.
- Detail refresh now holds one request guard across the initial status and peer
  requests, and later combined refreshes. Destroyed routes ignore late replies;
  file selection pauses updates. Temporary errors resume on later polling ticks;
  authorization errors stop the timer. Task-service response checks reject empty,
  wrong-GID or malformed task results and non-array peer results before processing.
  A failed peer subresult preserves valid task data and the previous peer display.
  This does not add transport cancellation, retry backoff or mutation replay.
- UI-01 charts consume a data reference/revision instead of deep-watching samples.
  Monitor writes increment the revision. Chart destruction detaches resize handlers;
  popover destruction removes handlers, plugin resources and its chart. Monitor
  storage retains at most 32 task histories plus global history, with least-recently
  used task eviction and existing 300/120 sample limits. A page-lifetime opaque
  RPC-session namespace separates histories without embedding destinations or
  credentials; existing profile switches reload the page. Histories are volatile,
  so this needs no saved-settings migration. The bound is a client retention policy,
  not a workload capacity claim.

Direct-mode lists still filter/sort in Angular digests and render all matching
rows. Gateway-mode lists render at most 100 summaries and bypass those filters.
Selection and keyboard actions apply to the displayed page; page/search/order
changes clear selection. Waiting drag uses absolute page offsets and is disabled
for search/reversed/custom orders. Direct waiting/stopped requests still default
to 1,000 items; direct active tasks have no equivalent bound.

The compatibility implementation fixes destinations, upstream credentials and
browser bearer authentication in private configuration, with per-scope server
authorization. It bounds snapshots/payloads, deadlines, cancellation, freshness
and admission. Slow-reader connection/write limits require a reverse proxy.
Direct HTTP/WebSocket RPC and local settings remain available; gateway HTTP
profiles explicitly opt in. The optional Compose gateway image and same-origin
Nginx route support local integration. Static SSH deployment remains separate.

The versioned compatibility API, authentication and signal handling are implemented.
Production service installation, readiness and binary/frontend rollback packaging
remain open. No production destinations have been selected.
