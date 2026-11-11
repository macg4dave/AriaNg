# Deployment

Stage 3 is complete for static releases to a Linux host over SSH. The Rust
operator tool in `tools/deploy/` implements locked install/build, artifact
validation, dry-run, transfer, atomic activation, content health checks and
rollback. The disposable Debian/OpenSSH/Nginx integration test validates this
contract. No production destination was contacted or selected.

The operator can run on Windows or Linux with Rust 1.85+ (tested with 1.99.1),
Node/npm, OpenSSH `ssh`/`scp` with SFTP support and `tar`. The destination requires
a shell, GNU coreutils, tar, curl, SSH/SFTP access, and a pre-created, dedicated,
empty release root owned by the deployment account. Its HTTP server must serve
`<release_root>/current` and allow atomic symlink replacement on that filesystem.
Hosting, TLS, browser authentication and aria2 remain operator-managed services.

## Private configuration and commands

Supply a private UTF-8 key=value file outside the checkout (ignored `.local/` is
a fallback). Blank lines and lines beginning `#` are permitted. Unknown/duplicate
keys, missing values and invalid paths fail before destination contact.

```text
ssh_alias=<operator-managed-ssh-alias>
release_root=/<dedicated-release-root>
health_url=https://<public-origin>/<index-path>
known_hosts=<absolute-private-known-hosts-file>
ssh_config=<absolute-private-ssh-config-file>
```

Replace placeholders externally. SSH aliases allow letters, digits, dots, hyphens
and underscores and must start with a letter/digit. The absolute Linux release
root allows letters, digits, slashes, underscores and hyphens, with no empty,
`.` or `..` components or trailing slash. Ancestor symlinks are rejected remotely.
SSH configuration and known-hosts files must be absolute regular files. Configure
the account, port and identity in that SSH config; use an SSH agent where possible.
Host verification and batch mode are enforced. Never disable host-key checking.
Health URLs support HTTP/HTTPS with a DNS or IPv4 host and optional valid TCP
port; userinfo, fragments and whitespace are rejected before destination contact.

From the repository root:

```sh
cargo run --release --locked --manifest-path tools/deploy/Cargo.toml -- dry-run --config <private-config>
cargo run --release --locked --manifest-path tools/deploy/Cargo.toml -- deploy --config <private-config>
cargo run --release --locked --manifest-path tools/deploy/Cargo.toml -- rollback --config <private-config>
```

Dry-run validates local configuration, describes phases without destination
values, and never contacts or modifies the destination. Normal deploy runs
`npm ci --no-audit --no-fund` and `npm run build` once before remote modification.
For an already reviewed/built artifact, append `--prebuilt <artifact-directory>`
to deploy or dry-run; dry-run then validates that artifact too. Rollback does not
install dependencies or rebuild. Tool output omits external command output and
private configuration; failures report a phase, not raw SSH/backend errors.

## Release lifecycle

The receiver initializes a version marker only inside an empty dedicated root.
An exclusive lock covers preparation, transfer and activation; concurrent
operations fail. Only approved static files are archived: index, license,
standard icons/robots, scripts, styles, language text and fonts. Unexpected
directories/files, symlinks, source maps and artifacts above 64 MiB are rejected.
The remote also rejects archive traversal and non-file/directory entries.
This structural validation does not detect a secret hardcoded in an approved
source asset: source review remains required.

Releases occupy `releases/r-<unique-id>`; `current` and `previous` are validated
relative symlinks to releases inside this root. Transfer/extraction happens before
activation. A temporary symlink is renamed over `current` atomically. The health
URL must return the active release's exact `index.html` content (no login page,
HTML rewriting or stale proxy response). curl has a ten-second deadline and a
2 MiB response bound. Failed activation health restores the prior active link,
or removes the new link for an initial deployment. Explicit rollback health-checks
the previous release and restores the starting link if that check fails.

No releases are recursively deleted automatically. Keep enough disk space and
remove old inactive releases through an operator-reviewed retention process.
Failed uploads/extractions can leave inactive diagnostic files. Interrupted
operations may leave `.deploy-lock`: inspect its owner and ensure no deployment
is running before removing that specific stale lock privately. A normal transfer
failure releases its own lock. Remote tool deadlines and SSH keepalive/connect
timeouts bound ordinary failures; an interrupted operator cannot guarantee
completion of remote cleanup. Do not run a second deployment to bypass a lock.

## Verification and boundary

```sh
cargo fmt --manifest-path tools/deploy/Cargo.toml --check
cargo clippy --locked --manifest-path tools/deploy/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path tools/deploy/Cargo.toml
cargo build --release --locked --manifest-path tools/deploy/Cargo.toml
cargo build --locked --manifest-path tools/deploy/Cargo.toml
cargo test --locked --manifest-path tools/deploy/Cargo.toml -- --ignored --nocapture
```

The explicit ignored test builds a disposable Linux SSH/Nginx container, generates
temporary keys under ignored `.local/`, pins the container's host key, and tears
the container down. It checks unchanged destination after dry-run, A/B releases,
atomic activation, explicit rollback, health failure recovery, private-file
rejection, failed builds, untrusted host-key rejection, SFTP transfer failure and
lock recovery, and the complete locked
install/build/deploy command. Production access is neither needed nor authorized.

The source checkout, `.git`, `node_modules` and private configuration are never
uploaded. Build-time variables and browser-delivered files cannot conceal secrets.
Browser RPC settings remain local. The proposed Rust gateway is not packaged by
this static tool: gateway adoption must extend the contract for compatible binary
versions, process lifecycle, private runtime config and gateway health/rollback
under GATE-05. Stage 3 completion applies to the current static architecture.
