# Local Docker testing

For the optional Rust gateway image, private runtime configuration and same-origin
Nginx integration, see [Gateway integration](GATEWAY-INTEGRATION.md). The normal
Compose command below continues to support direct aria2 without that service.

The test image builds the standard frontend with Node 24 on Debian Bookworm,
then copies only `dist/` into a Debian Bookworm image running packaged Nginx.
Node, npm dependencies, Git, and the source checkout are absent from the final
image. A second Debian container runs packaged aria2 as `www-data`. Nginx forwards
`/jsonrpc` to this disposable aria2 service, including WebSocket upgrades. aria2
requires a random RPC token generated outside tracked files; it has no published
host port. This proxy is for local testing, not production authentication design.

## Start and stop

From the repository root with Docker running in Linux-container mode:

```sh
python tests/run_docker.py
```

Open <http://localhost:8080>. The published port binds to loopback only.

The command creates `.local/aria2/rpc-secret` if missing (without printing it),
builds/starts both services, waits for health, and runs HTTP plus real RPC tests.
The ignored secret is mounted read-only into aria2, then written into a private
configuration file on tmpfs at startup; it is absent from image layers and process
arguments. Never commit or export browser settings containing this token.
On POSIX hosts the secret's parent directory is mode 0700; the file is read-only
and readable by the container user through Compose's bind-mounted secret.

To prepare credentials and start without tests:

```sh
python tests/run_docker.py --setup-only
docker compose up --build -d --wait
```

For manual AriaNg use, set its RPC host/port to the same local web origin, path
`jsonrpc`, protocol HTTP, and secret to the value in the ignored local secret
file. Read the file locally without pasting its contents into shared logs or docs.
Automated browser tests configure a temporary profile themselves.

```sh
docker compose ps
docker compose logs web
docker compose down
```

Rebuild after source changes; no source directory is mounted. To build without
starting a container: `docker build -t ariang-test:local .`. Run the image through
Compose so the named aria2 upstream is available.

## Runtime and configuration

Nginx runs as `www-data` on container port 8080 with a read-only root filesystem,
a temporary writable `/tmp`, dropped capabilities, and no privilege escalation.
The health check requests `/index.html`; `/healthz` also exposes a simple liveness
response. Missing assets return 404. Assets revalidate to avoid stale test builds.
Request access logging is disabled to avoid recording sensitive URL parameters;
Nginx errors go to stderr and should be redacted before sharing. aria2 uses bounded
temporary download storage (64 MiB) and temporary configuration storage. Recreating
its container discards downloads and task state. Task history and temporary
downloads are not persisted across container stops or restarts.

The Docker build context uses an allowlist in `.dockerignore`. Only required
source/build inputs are sent; private local settings and keys are excluded. Source
files must still obey the repository's prohibition on embedded private details.
Do not supply secrets through build arguments or frontend environment injection.

aria2's internal Docker network blocks external downloads; Nginx also joins a
host-facing network for loopback web access. Tests download only the
64 KiB zero-byte fixture served by Nginx at `/test-fixture.bin`. Docker service
names are used only inside this disposable network; the browser uses the local
web origin for RPC. Do not use production credentials or destinations here.

This setup adds no TLS, login service, production rollout, or capacity guarantees.
Production deployment and rollback remain separate work in `DEPLOYMENT.md`.
Base tags and Debian packages receive updates; use `docker compose build --pull`
to refresh them. The existing npm lockfile controls application dependencies.

Configuration references: [Node image variants](https://hub.docker.com/_/node)
and [Nginx HTTP directives](https://nginx.org/en/docs/http/ngx_http_core_module.html).

## Verification

Built and smoke-tested on Docker Desktop with Linux/amd64 containers: Compose
health, `nginx -t`, HTTP index and its 13 referenced static assets, a translation,
license, liveness, and 404 responses for missing/private paths passed. Verified
Debian 12, `www-data`, and absence of Node, Git, and build dependencies from the
runtime.

Repeat the HTTP smoke checks with Python 3 (standard library only):

```sh
python tests/docker_smoke.py
```

Optional browser checks require the Python `playwright` package in your test
environment and a local Edge installation:

```sh
python tests/docker_smoke.py --browser --channel msedge
```

Alternatively, with the optional JavaScript Playwright installation described
in DEVELOPMENT.md, run `node tests/browser-smoke.cjs` for the same six-route,
theme/language persistence, translation, mobile and runtime/asset checks. This
does not require the Python Playwright package.

Chrome can be selected with `--channel chrome`. Tests use a new browser context
and block outbound RPC traffic; no existing browser profile or credentials are
used. Browser checks cover six routes, light/dark themes, persisted theme and
French language selection, translated content, a 390px mobile viewport, uncaught
JavaScript errors, and failed local resource responses.

Run the complete suite, including real RPC and browser-to-aria2 integration:

```sh
python tests/run_docker.py --browser --channel msedge
```

RPC tests verify missing/wrong token rejection, version/stats, paused queue entries,
start/pause/resume, completion, SHA-256 and downloaded bytes, task removal, and
HTTP failure reporting. The browser test submits a real fixture download through
AriaNg and verifies the completed task appears. Tests remove their RPC task
records; downloaded fixture files remain disposable until container recreation.

GitHub Actions runs `.github/workflows/docker-tests.yml` on pushes and pull
requests with Chromium and tears down the environment afterward. No repository
or CI credentials are needed. Run `docker compose down` locally when finished.
These are functional integration checks, not production capacity benchmarks.
BitTorrent, FTP/SFTP, and session persistence are not yet
covered by automated integration checks.

The full local suite passed with headless Edge, including authenticated HTTP and
WebSocket RPC and an AriaNg-submitted download. GitHub Actions is configured but
has not been executed from this local session.
