# Completed preparation baseline

Stages 1-2 are complete for the current static AngularJS application and the
reproducible synthetic workloads below. This is a development baseline, not a
production capacity certification. New components and affected behavior require
additional baselines as implementation proceeds. Stage 5 owns larger-load targets
and validation; the reported production freeze remains unreproduced locally.

## Core behavior inventory (BASE-01)

| Area | Current source boundary | Compatibility checks and known limits |
| --- | --- | --- |
| Task lists | `controllers/list.js`, `views/list.html`, `filters/taskOrderBy.js`, search/selection in `core/root.js` | Active/waiting/stopped, names/counts, filtering, sorting, stable GID rows, selection and waiting queue drag positions must remain. Synthetic row/search/mobile checks exist; drag and complete keyboard parity remain required when changing list rendering. |
| Task controls | `controllers/main.js`, `controllers/new.js`, `services/aria2TaskService.js` | Add URI/torrent/metalink, start/pause/remove/retry, move queue, copy links and select files. Disposable HTTP download checks cover URI submission, pause/resume, removal, completion/checksum and failure. BitTorrent/FTP/SFTP integration is not covered. |
| Task details | `controllers/task-detail.js`, task service, piece/peer directives | Overview, pieces, files, peers and options. Deterministic refresh checks cover task/peer cycles, malformed results, editing and late callbacks; live HTTP/WS paused-task routes are checked. Real peer load remains open. |
| RPC | HTTP/WS services and `aria2RpcService.js` | JSON-RPC method/parameter compatibility, token handling, read polling, failure/reconnect, notifications and mutation callbacks. Existing 20-second deadlines and one outstanding list/global/detail cycle form the baseline. No automatic mutation replay. |
| Multiple servers | `ariaNgSettingService.js`, server actions in `core/root.js` | Current profile is used to construct transports; switching profiles reloads the page. Keep saved profiles and isolate credentials, requests, selection and histories. Gateway profiles must be explicit opt-in; never silently migrate direct profiles. |
| Settings/data | Setting/storage services, AriaNg settings controller | Browser storage, import/export, current RPC and extended servers, language/theme/display/refresh options. Exports can contain credentials. Existing data format is preserved; any migration needs explicit tests and documentation. |
| UI compatibility | `core/router.js`, `views/`, `styles/`, `langs/` | Six smoke-tested routes, persisted light/dark and language choices, translation, mobile width and browser errors. Existing Docker smoke suite is the integration reference; affected changes need focused checks. |
| Hosting/security | Static build and separate aria2/web server | Standard assets and single-HTML formats, upstream attribution and MIT license. Static assets cannot hide backend secrets. Deployment ships only reviewed public artifacts; authentication stays with infrastructure until a gateway is implemented. |

Inventory completion means the boundaries, preserved behavior, existing checks and
coverage gaps are identified. It does not mean every protocol and interaction has
automated coverage. See DEVELOPMENT.md for runnable commands and limitations.

## Reproducible workload baseline (PERF-01)

All inputs are synthetic or disposable. No private server inventory is used.

| Workload | Parameters | Recorded evidence |
| --- | --- | --- |
| Processing | One client/list, 1,000 tasks, 1/100 files per task, initial full data then 30 cached/basic refreshes after warmup | `task-processing.cjs`; comparable pre-change and updated timings in PERFORMANCE.md |
| List DOM | 25/250 tasks, ten object replacements, search and narrow viewport | `list-browser.cjs`; stable rows and digest timing |
| Transport delay | HTTP and WS, 20/25 active task records, 1/100 files, 0/1500 ms replies, one-second polling | `stall-browser.cjs`; response bytes, fixture latency, digest/frame scheduling, observed fresh values, long tasks and request overlap |
| Real aria2 | One browser at a time; 20 paused tasks, then 25 tasks including four active single-file transfers at 1 KiB/s, zero peers; four list replies, disconnect/recovery and two detail refreshes | `live-stall-browser.cjs`; transport latency/bytes, browser scheduling, recovery and fixture cleanup |

Environment, exact commands, measured values and limits are in PERFORMANCE.md.
No numeric latency/resource budget or production concurrency target is agreed.
No throughput or memory-capacity improvement is claimed. Long-duration load,
BitTorrent peers, synchronized upstream resource telemetry and production symptom
diagnosis remain stage 4/5 work rather than verified baseline results.
