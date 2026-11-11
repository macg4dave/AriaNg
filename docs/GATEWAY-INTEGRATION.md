# Gateway integration

The optional Rust gateway now has an explicit browser RPC mode, compact task
pages and an optional local Compose service. Direct aria2 HTTP/WebSocket profiles
remain the default. Existing saved profiles need no migration. New `rpcMode`
values are retained through profile cloning, switching, import and export.

## Browser configuration

In AriaNg Settings, select the relevant RPC tab. Select Gateway under RPC Mode,
use the current page's HTTP/HTTPS origin and port, set RPC Interface to
`v1/servers/<scope-id>/rpc`, and enter the scope's browser bearer token in Gateway
Browser Token. Reload after changing connection settings. Gateway requests use
POST regardless of the saved direct-mode HTTP method. The token is sent in the
Authorization header, never as an aria2 parameter. The gateway mode rejects
other origins before sending credentials; host spelling and ports must match
the page origin. Use a same-origin reverse proxy. Browser bearer tokens remain
visible to their browser user; they are separate from upstream aria2 secrets.

Each list renders at most 100 summaries. Search and display order apply on the
server before pagination; previous/next controls show the filtered total. Selection,
Select All and keyboard actions apply to the displayed page. Changing page,
search or order clears selection. Refreshes on the same page retain selected GIDs.
This explicit page-scoped selection replaces whole-list selection in gateway mode.
Direct mode retains its existing list behavior.

Detail and settings requests still use compatible aria2 RPC. Compact rows retain
file counts, torrent identity, info hash and an unambiguous single download URL
for existing actions, without transferring full file arrays. Detail links remain
available on mobile. Waiting-list drag is supported in default ascending order
without search; page offsets are included in absolute queue positions. Other
orders/search disable drag to avoid ambiguous queue placement.

One list request may be outstanding per controller. Search/order changes discard
obsolete responses and request the latest query when the pending read settles.
Page transitions include the last snapshot revision; an expired revision triggers
one fresh read without that revision. Polling obtains current snapshots. Leaving
a route discards late results. The gateway fails inventories above 1,000 tasks
explicitly rather than silently truncating them.

RPC errors use translated, escaped, nonmodal notifications. They no longer place
an overlay over navigation. HTTP 200 JSON-RPC errors and malformed/non-JSON HTTP
replies are failures and settle the request guard. Authorization failures stop
list/global polling; connection settings remain accessible.

## Optional local Compose runtime

Create a private TOML file outside the checkout (ignored `.local/` is a local
fallback). All values below are placeholders:

```toml
listen = '<listen-address>:8081'
allow_network_listener = true
browser_origin = '<exact-browser-http-origin>'
[[servers]]
id = '<scope-id>'
upstream = '<aria2-http-rpc-url>'
upstream_secret = '<private-upstream-token>'
browser_token = '<distinct-private-browser-token>'
```

For the disposable Compose fixture only, the gateway listens on all container
interfaces at port 8081, and connects to the `aria2` service on port 6800 at
`/jsonrpc`. Use the fixture's existing ignored upstream token and a separate
random browser token. Do not use production credentials. Set `GATEWAY_CONFIG_FILE`
to the private TOML's absolute path in the current shell, then run:

`allow_network_listener` defaults to false: ordinary configurations still require
a loopback listener. Enable it explicitly only for a restricted container/private
network behind the reverse proxy. The optional Compose service publishes no port.

```sh
docker compose -f compose.yaml -f compose.gateway.yaml up --build -d --wait
```

The gateway image builds a locked Linux release and runs as www-data with a
read-only root, no published port, dropped capabilities and resource limits.
Its build context allowlists Rust source/manifest/lockfile only. Private config is
mounted as a Compose secret at runtime. Nginx proxies `/v1/servers/` to the optional
service with body, connection and transport timeout limits; the default direct
RPC route continues working without the gateway service. The gateway health check
tests process HTTP liveness, not upstream readiness or successful authorization.

The static SSH operator does not install this service or atomically couple its
binary version with a frontend release. Production service management and paired
rollbacks remain a separate deployment requirement. This local integration does
not publish or activate anything in production.

## Checks

```sh
node tests/http-rpc.cjs
node tests/browser-smoke.cjs
node tests/gateway-ui.cjs
cargo test --locked --manifest-path server/gateway/Cargo.toml --test live -- --ignored --nocapture
```

The UI fixture covers 250 synthetic tasks, bounded pages, search/order, selection,
absolute queue positions, stale-revision recovery, slow reads, route cleanup and
unauthorized navigation. The Rust launcher tests two browsers against the actual
gateway listener and disposable aria2. `tests/gateway-browser.cjs` also supports
`GATEWAY_FIXTURE_PROXY=1` with an explicitly supplied fixture origin, endpoint and
tokens to exercise the optional Nginx/Compose gateway instead of its route bridge.
Never print those environment values or use production services for these checks.
