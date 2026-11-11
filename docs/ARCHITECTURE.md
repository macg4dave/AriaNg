# Architecture

## Current system

This fork starts from AriaNg: an AngularJS browser application built with Gulp.
The standard build produces static assets in `dist/`; the all-in-one build embeds
assets in a single HTML file. There is no application backend written in Go or
Rust in this repository yet.

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
  workload and maintainability. Rust is requested for the proposed polling gateway;
  no server runtime is implemented yet. Browser code and deployment glue use appropriate tools.
- Rewrites are welcome if affected core functionality is preserved and settings
  or user data have a migration path when needed. Preserve upstream attribution.
- Keep service boundaries simple. Add a backend for a concrete requirement;
  existing aria2 and hosting do not require replacement merely for language choice.
- Private infrastructure configuration remains external. Public browser assets
  cannot conceal credentials or destination addresses from browser users.
- If hidden backend credentials or addresses require a proxy, its design must
  provide authentication, authorization, externally configured destination
  restrictions, and isolation between users and servers. This proxy is a proposal,
  not an implemented component.

## Decisions still open

### Rust backend assessment (2026-11-11; proposed)

The requested server language for this optimization work is Rust. No backend has
been added in this first pass. The concrete candidate is a same-origin RPC gateway
that owns bounded upstream polling per authorized server, derives compact task
summaries, and offers filtered/sorted pages or snapshot updates to clients. This
could remove duplicated polling across browser clients and avoid delivering full
file inventories for list rendering. Rust fits bounded snapshot storage and CPU
work on large inventories; performance remains to be measured.

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

The current list still filters/sorts in Angular digests and renders all matching
rows. A backend alone cannot remove DOM/digest cost. Bounded rendering needs a
separate compatible design for selection, keyboard actions, and waiting-list drag
positions. Waiting/stopped requests currently default to 1,000 items; active tasks
have no equivalent limit. Pagination must preserve ordering and task controls.

Before connecting the proposed gateway, define external destination allowlists,
upstream credentials, browser authentication and per-user/server authorization.
Never accept arbitrary browser-supplied upstream URLs or forward tokens through
public config. Define bounded snapshots/payloads, deadlines, cancellation, freshness,
failure status, slow-client behavior, and deployment/rollback with frontend version
compatibility. Direct HTTP/WebSocket RPC and existing local settings must remain
available, with explicit opt-in or migration for gateway profiles. Sorting/search
semantics and selected task operations need integration parity checks. Static
deployment remains the implemented contract.

The proposed gateway's API contracts,
hosting OS, service lifecycle, authentication approach, and deployment transport
remain to be selected. Do not treat these as implemented or invent server details.
