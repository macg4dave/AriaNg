# Working on this fork

## Purpose

Customize AriaNg for the owner's servers, branding, and workflows while keeping
deployment simple. Prioritize speed and reliable operation under larger loads.
Preserve upstream attribution and the MIT license. Rewrites are welcome when they
preserve core functionality and have a clear engineering benefit.

## Project context and documentation

Read [docs/README.md](docs/README.md) before beginning work, then read the linked
architecture, roadmap, backlog, development, performance, and deployment documents
relevant to the task. Keep current state and proposed work distinct. Update the
appropriate document when behavior, architecture, commands, or task status changes.

Keep repository facts and plans in those documents. Language prompts must remain
repository-neutral and refer the assistant to project documentation rather than
embedding its contents. A backlog entry alone does not authorize execution.

## Working conventions

- Read relevant code before editing; follow existing AngularJS patterns when
  working within the current frontend.
- Use four-space indentation, single quotes in JavaScript, semicolons, UTF-8, and LF
  as defined by `.editorconfig` and `.eslintrc.json`.
- Framework migrations and rewrites are allowed when they support the fork's
  goals and preserve core functionality. Keep their scope purposeful and verify
  compatibility; avoid unrelated churn. Keep lockfiles in sync with dependencies.
- Put user-facing strings through the existing translation mechanism.
- Preserve responsive layouts, themes, and existing multi-server behavior unless
  the requested customization explicitly changes them.
- Keep instructions generic. Never put actual infrastructure identifiers in docs,
  examples, test fixtures, screenshots, commit messages, or workflow files.

## Server languages and architecture

- Prefer Rust over Python. If a task can be implemented in Rust, implement it in
  Rust. This includes test automation, development tools, deployment tooling, and
  scripts. Do not introduce Python implementations or new Python dependencies
  where Rust can do the work. Existing Python tooling may remain until migrated;
  use Rust for new work and replacements. This preference does not remove Go as
  an approved language for application server components.

Reusable task prompts for either Codex or Claude: [Go](docs/prompts/GO.md) and
[Rust](docs/prompts/RUST.md). Use the appropriate prompt with a concrete task;
these are guidance, not an instruction to start both implementations.

- Write new or rewritten application server components in Rust or Go. Choose per
  component based on its workload, operational needs, and maintainability; explain
  the choice briefly. Neither language is mandatory for every component.
- Favor Rust where tight control of memory, allocations, or CPU-intensive work
  justifies it. Favor Go where straightforward network services, concurrency, and
  operational simplicity fit the workload. Validate performance assumptions with
  measurements rather than treating language choice as proof of speed.
- This policy applies to application server code. Browser code and build/deploy
  glue may use tools suited to those environments. Existing aria2 and web-server
  infrastructure do not need replacing solely to comply with this policy.
- Consult `docs/ARCHITECTURE.md` for the current component inventory. Add server
  components when a concrete requirement warrants them; keep service boundaries
  simple and avoid unnecessary services or language mixing.
- Keep private configuration and credentials on the server or in external secret
  stores, following the configuration boundary below.

## Rewrites, speed, and larger loads

- Before a rewrite, identify the affected core behavior and establish a baseline.
  Preserve task management, RPC compatibility, multi-server connections, settings,
  translations, responsive layouts, and themes where affected. Preserve user data
  and settings, or provide an explicit migration path.
- Use meaningful behavior and integration checks to establish compatibility.
  Prefer incremental replacement with a working application at each stage when
  practical. Document intentional behavior changes and migration requirements.
- Define a representative workload before optimizing: concurrent users and RPC
  connections, task counts, update frequency, payload sizes, and slow clients as
  relevant. Use synthetic or sanitized data without real server details.
- Measure before and after under comparable conditions. Report throughput,
  latency (including p95/p99 where relevant), errors, CPU, and memory for server
  changes; measure rendering responsiveness and network use for frontend changes.
  Record the workload and limits of the results. Do not invent capacity targets
  or claim improvements that were not measured.
- Bound concurrency, queues, request sizes, and memory growth. Use timeouts,
  cancellation, backpressure, and bounded retries for network services. Check
  overload behavior and resource recovery, not just peak throughput.
- Reduce unnecessary RPC polling, duplicate work, and large-list rendering costs
  where measurements show a bottleneck. Caching and batching must preserve data
  freshness, server isolation, and correctness.
- Performance work must preserve authentication, authorization, secret handling,
  and correctness. Add useful observability without logging sensitive values.

## Secrets and server information

- Never commit private keys, credentials, RPC tokens, passwords, certificates with
  private material, real hostnames, IP addresses, usernames, or server paths.
- Keep deployment values outside the checkout in environment variables, an SSH
  config, a secret store, or a private configuration file. Use placeholders only in
  tracked examples. A private repository is not a secret store.
- Local fallbacks such as `.env`, `.env.*` (except `.env.example`), `.local/`,
  `*.local.json`, and assistant-local instruction/settings files are ignored.
  Ignore rules are a backstop: they do not remove already tracked files or history.
- Do not read, print, copy, or upload private configuration unless needed for an
  authorized operation. Redact command output and deployment errors before sharing.
- Never embed credentials or private server details into frontend code, HTML,
  source maps, generated assets, or public runtime JSON. Browser-delivered data is
  visible to users; build-time environment variables do not make it secret.
- Configure browser RPC connections locally where appropriate. If private backend
  credentials must stay hidden, design a server-side authenticated proxy; do not
  claim that a static frontend can hide them.
- If sensitive material is discovered in tracked files, stop propagating it,
  report its location without reproducing the value, and recommend revocation or
  rotation. Do not rewrite Git history without explicit authorization.

## Verification

Use [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for source layout, current commands,
and verification limitations, and [docs/PERFORMANCE.md](docs/PERFORMANCE.md) for
measurement requirements and results. Run checks appropriate to the change and
state any checks that could not be performed. Before finishing, inspect the diff
for secrets, accidental generated output, and unrelated changes. Summarize the
change, validation, and remaining limitations.

## Deployment direction

See `docs/DEPLOYMENT.md` for the intended deployment contract. Deployment tooling
status is recorded there. Build portable artifacts appropriate to the current
architecture; supply destinations and credentials externally. Prefer a preview/dry-run, separate release directories,
an atomic activation step, a health check, and rollback. Never deploy the checkout,
local configuration, or `node_modules` to the web root.

Work autonomously on reversible implementation and validation. Treat publishing
or production changes according to the user's authorization; do not infer a
production rollout request from a request to edit code or prepare deployment.
