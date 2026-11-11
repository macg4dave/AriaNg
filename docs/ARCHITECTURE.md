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
  workload and maintainability. No component language or server framework has
  been selected yet. Browser code and deployment glue use appropriate tools.
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

The first server-side use case, Go versus Rust for that component, API contracts,
hosting OS, service lifecycle, authentication approach, and deployment transport
remain to be selected. Do not treat these as implemented or invent server details.
