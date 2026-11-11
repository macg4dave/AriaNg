# Rust RPC gateway

`server/gateway/` implements the compatible transport/shared-polling stage. Rust
follows the selected direction and provides explicit snapshot ownership; it is
not evidence of better performance. aria2 remains the download engine. Existing
AngularJS HTTP profiles work without saved-settings migration.

## Private configuration and startup

Supply an external TOML file, maximum 64 KiB. Replace placeholders privately;
unknown fields fail validation. Never place the configured file in public assets.

```toml
listen = "<numeric-loopback-address>:<gateway-port>"
browser_origin = "https://<public-origin>"

[[servers]]
id = "<opaque-server-id>"
upstream = "https://<private-upstream>/<rpc-path>"
upstream_secret = "<private-aria2-token>"
browser_token = "<independent-random-browser-credential>"
```

The listener must be loopback. Origin must be exactly scheme, host and optional
port, without path/trailing slash. Upstreams are fixed HTTP/HTTPS URLs without
userinfo, query or fragment. Redirects and environment proxies are disabled;
normal TLS validation applies. Prefer HTTPS for upstream traffic leaving the host.
No browser request chooses a destination.

Configure 1-16 scopes. IDs contain 1-64 ASCII letters, digits, hyphens or underscores.
Each scope has separate snapshots and a unique browser credential. Generate each
credential privately with a cryptographic generator, using at least 32 random
bytes encoded as hex. Accepted text is 32-256 printable ASCII characters; length
validation cannot establish entropy. Upstream tokens must be nonempty.

A browser credential grants full RPC control of exactly one scope, including
global settings and shutdown. Distribute it only to users authorized for that
access. Shared possession is explicit shared authorization. Use separate scope
entries/credentials for different security contexts even at the same destination;
snapshots never cross scopes. There is no anonymous mode, viewer role or identity
inferred from proxy headers. Rotate/revoke by restarting with changed private
configuration. Browser credentials are visible to their users; upstream tokens
remain on the gateway.

```sh
cargo build --release --locked --manifest-path server/gateway/Cargo.toml
cargo run --release --locked --manifest-path server/gateway/Cargo.toml -- <private-config.toml>
```

Run the resulting `ariang-gateway` executable with the external configuration path
as its sole argument. `GET /healthz` returns `ariang-gateway/v1`, indicating process
liveness rather than upstream readiness. Errors omit private config/destinations
and raw backend messages. The service has no request/body logging.

## Browser opt-in and hosting boundary

Use the same-origin HTTPS proxy that serves the UI. Forward `/v1/servers/` to the
loopback gateway, preserving path and Authorization. Apply proxy body/header/
connection limits and read/write deadlines. Do not log credentials or RPC bodies.
The gateway does not serve static assets or terminate browser TLS. CORS is not
enabled. Origin, when present, must match the configured origin; native clients
can omit Origin but still need the bearer credential.

Add a profile through the existing AriaNg RPC settings:

- Protocol: HTTP/HTTPS matching the UI origin; method: POST.
- Host/port: the UI origin; interface: `v1/servers/<opaque-server-id>/rpc`.
- RPC secret: empty; never enter the upstream token here.
- RPC request headers: `Authorization: Bearer <browser-credential>`.

Keep existing direct HTTP/WebSocket profiles for switching back. The gateway
supports HTTP POST only, not GET or WebSocket. No automatic migration/activation
occurs. Local settings exports include the browser credential and remain private.

## Compatibility, freshness and failures

`POST /v1/servers/{id}/rpc` accepts JSON-RPC 2.0 requests with IDs and positional
parameters. It preserves IDs/result shapes for current UI methods: task controls,
detail/files/peers, options, torrent/metalink uploads and `system.multicall`.
String request IDs are limited to 128 UTF-8 bytes to bound error envelopes.
An explicit allowlist rejects unknown methods, browser `token:` parameters,
notifications, top-level batch arrays and nested multicalls. All multicall inputs
are validated before execution. Results retain aria2's `[result]`/fault wrapping.
Execution is ordered, not transactional: an overall timeout or response limit may
leave earlier mutations applied. Never blindly repeat a timed-out mutation.

`tellActive`, `tellWaiting`, `tellStopped` and `getGlobalStat` share snapshots by
exact method/parameters within a scope. Projections and offsets/counts use separate
slots. One polling task per scope refreshes recently accessed slots. The first
read, or a read of expired data, fetches synchronously; concurrent requests
coalesce under the scope gate. Freshness is one second after fetch completion,
plus upstream fetch duration. Failed/expired snapshots are never served as known
success. `X-Snapshot-Age-Ms` measures time since completion (maximum component age
for multicall, zero for uncached results); this is not an atomic cross-method
snapshot. External aria2 changes appear at the next refresh.

Mutations invalidate all scope snapshots before transmission, including ambiguous
timeouts. Mutations are never polled, replayed or retried. Detail reads are not
cached. All RPC responses use `Cache-Control: no-store`. HTTP errors are 401 for
auth, 400 for malformed/unsupported calls, 503 for admission/slot pressure, and
502 for upstream failures, with JSON-RPC faults. Raw upstream error messages are
replaced with generic faults. Normal successful task/option data retains download
paths and user-supplied URLs: scope access grants that information and full control.

## Resource bounds and lifecycle

| Resource | Limit |
| --- | --- |
| Configured scopes / polling tasks | 16 |
| Admitted RPC requests including body read and queued work | 32 process-wide; excess fails immediately |
| Concurrent upstream calls | One per scope, at most 16 process-wide |
| Cached signatures | Eight per scope; excess fails rather than grows |
| Idle snapshot retention | 30 seconds |
| Request, upstream response, aggregate browser response | 2 MiB each |
| Cached signature | 4 KiB |
| Multicall width / positional arguments | 64 / 16 |
| Upstream connect / complete request deadline | 3 / 10 seconds |
| Complete HTTP operation including body read/queue wait | 15 seconds |

These are safety bounds, not capacity targets. Encoded cached content is bounded
by 16 × 8 × 2 MiB; decoded JSON, transport buffers, copies and allocator overhead
increase RSS, which has not been characterized. Large inventories/uploads may
fail the cap; direct profiles remain available. Full list data is preserved rather
than silently truncating tasks/files. Browser list processing and rendering remain.

Polling skips missed ticks, never overlaps within a scope, releases the gate
between slots and evicts idle entries. Failure retries happen only through later
read polls at the fixed interval/deadline. There is no push queue or mutation retry.
Admission bounds FIFO waiters; overload can cause queue deadlines to expire.
Cancellation drops pending upstream futures but cannot undo received mutations.
Ctrl-C (and SIGTERM on Unix) cancels polling, stops accepting connections and drains
admitted work within its deadlines. Proxy limits must also bound idle TCP
connections and slow readers: application admission does not bound transport
connections/write time.

## Verification and remaining scope

```sh
cargo fmt --manifest-path server/gateway/Cargo.toml --check
cargo clippy --locked --manifest-path server/gateway/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path server/gateway/Cargo.toml
cargo build --release --locked --manifest-path server/gateway/Cargo.toml
cargo test --locked --manifest-path server/gateway/Cargo.toml --test live -- --ignored --nocapture
cargo test --release --locked --manifest-path server/gateway/Cargo.toml --test load -- --ignored --nocapture
```

The explicit live test requires healthy disposable Compose web/aria2, the existing
ignored fixture token, Node, optional Playwright and Edge from DEVELOPMENT.md.
A Rust launcher starts an ephemeral gateway and the JS runner bridges the UI's
same-origin route to its real HTTP listener. This substitutes for an installed
reverse proxy; it does not test production TLS/proxy configuration. It creates
one paused fixture task, checks two browser contexts, waiting/detail/mobile views,
multicall, task option changes and auth, then removes its task. No production
endpoint is involved. Other Rust tests use synthetic upstreams and credentials.
The explicit release load check compares direct/shared full-list delivery with
20 clients and synthetic 25-task/100-file inventories. It emits latency, request
counts/bytes, errors and combined test-process CPU/RSS; limits are in PERFORMANCE.md.

GATE-01 and compatible transport/shared reads from GATE-02 have source and checks.
Compact summaries (GATE-03), query/pagination/selection and push updates (GATE-04),
production binary/frontend packaging, readiness, service installation and atomic
rollback (GATE-05) remain unimplemented. Long peer/download soaks, real proxy
disconnect tests, Linux runtime validation, production resource/load measurements
and operator-chosen capacity targets remain open. The full roadmap is not complete.
