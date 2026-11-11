# Deployment plan for this fork

Status: remote deployment is design guidance only.
Local Debian/Nginx testing is available via [Docker Compose](DOCKER.md); remote
deployment automation and rollback are not implemented.

AriaNg builds to static files in `dist/`. Node.js is needed on the build machine;
the destination needs an HTTP server for the standard build. The aria2 service
and any authentication or reverse proxy are separate infrastructure.

New or rewritten application server components must use Rust or Go according to
the workload, following `AGENTS.md`. If introduced, deployment must package their
release binaries alongside the frontend artifact as appropriate, manage their
lifecycle, and coordinate compatible versions and rollback. The static-only
workflow below describes the current application; extend it when server code is
added. Keep server configuration external to both binaries and public assets.

## Intended operator workflow

1. Configure a private destination outside the checkout, preferably using an SSH
   alias and key managed by the operator's SSH agent. Keep actual addresses,
   accounts, paths, and keys out of this repository.
2. Run one deployment command with a private configuration path or external
   environment values. It should install locked dependencies and build once.
3. Preview the destination and intended changes with a dry-run option, without
   printing credentials or private addresses into shared logs.
4. Transfer only the built static artifact to a new release directory. Validate
   all required inputs before contacting the destination; abort on build or
   transfer failure. Verify SSH host keys using an externally managed known-hosts
   file; never disable host verification for convenience.
5. Check the uploaded release, switch the active release atomically where the
   hosting system supports it, and verify HTTP availability. If verification
   fails, restore the previous release and report failure.
6. Keep the preceding release for an explicit rollback command. Limit cleanup to
   verified release directories owned by this deployment; avoid broad remote
   deletion or synchronization with `--delete` against a live web root.

The eventual interface should support deploy, dry-run, and rollback without
editing source files for each server. Choose its implementation after confirming
the destination OS, hosting method, and available transfer tools. Do not assume
Docker, a particular web server, or GitHub Actions access to the server.

## Configuration boundary

Tracked files may contain generic deployment code, config schemas, and placeholder
examples. Actual SSH identities, destination names/addresses, remote paths, health
check URLs, and authentication belong in private external configuration. CI, if
used, must receive these through its secret store and avoid logging them.

An ignored `.local/` directory or `.env` file is a local fallback, not a guarantee
against disclosure. Do not package it. Never copy `.git/`, private settings, SSH
material, or the source checkout into the public artifact. Do not pass secrets to
frontend bundling or publish private server inventories as runtime JSON.

Browser connections necessarily expose their destination to the browser user.
For hidden backend addresses or credentials, use an appropriately authenticated
server-side proxy and a generic same-origin frontend endpoint. Browser-local RPC
settings and exported AriaNg settings can also contain sensitive values; keep
exports outside the repository.

## Acceptance checks for deployment implementation

- A clean checkout builds using the lockfile and produces the standard artifact.
- Missing or invalid private configuration fails before any remote modification.
- Dry-run does not modify the server or reveal private values in shared output.
- Upload/build failures leave the active release intact.
- Activation, health-check failure recovery, and explicit rollback are verified
  against a disposable test destination before a production rollout.
- The deployed artifact contains no private configuration or server-specific data.
- The operator can deploy and roll back using documented commands with external
  configuration, without changing tracked files.
