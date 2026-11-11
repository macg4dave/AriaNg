# Backlog

Items below are planned work, not authorization to execute everything at once.
Use IDs in task requests and update status with verified evidence as work lands.

| ID | Item | Status | Dependencies | Acceptance criteria |
| --- | --- | --- | --- | --- |
| TEST-01 | Debian/Nginx Docker test environment | Complete | Docker Linux containers | Image built; Compose service healthy; Nginx config valid; index, referenced assets, language file, license, and health endpoint served; missing/private paths return 404; runtime verified as unprivileged Debian |
| TEST-02 | Repeatable HTTP and browser smoke checks | Complete | TEST-01; Python; Playwright/Edge for UI | Clean build and tests passed; six routes, theme/language persistence, translation, mobile width, and uncaught browser errors checked with RPC blocked; live RPC remains separate work |
| TEST-03 | Disposable aria2 and automatic integration tests | Complete locally; CI configured | TEST-01; Docker; Python; browser tooling for UI | Real token auth rejection, HTTP/WebSocket RPC, task controls, verified download bytes/checksum, removal and failure reporting passed; AriaNg-submitted download shown completed; push/PR workflow configured but remote run unverified |
| BASE-01 | Inventory core behavior | Planned | None | Document affected task operations, RPC behavior, settings, multi-server isolation, and UI compatibility; identify meaningful regression checks |
| PERF-01 | Establish performance baseline | Planned | BASE-01 | Define reproducible synthetic workload, record baseline and tool commands, and distinguish provisional targets from agreed targets |
| DEPLOY-01 | Define deployment interface | Planned | Hosting requirements | Specify external config schema, target platform, transport, artifact contents, deploy/dry-run/rollback interface; include no private values |
| DEPLOY-02 | Implement deployment | Planned | DEPLOY-01 | Meet deployment acceptance checks in DEPLOYMENT.md against a disposable destination; document commands and verified rollback |
| ARCH-01 | Select first server component | Planned | Concrete use case; PERF-01 if performance-driven | Record component boundary, Go/Rust rationale, API and authentication needs, compatibility requirements, and deployment consequences |
| IMPL-01 | Implement selected component or optimization | Planned | Relevant architecture decision and baseline | Preserve affected behavior, pass meaningful checks, and demonstrate any claimed improvements with comparable measurements |
| LOAD-01 | Validate larger loads | Planned | PERF-01; relevant implementation | Report throughput, tail latency, errors, resource use, overload behavior, and recovery against agreed targets |

No application, benchmark, or deployment implementation is marked complete by
the creation of this documentation.
