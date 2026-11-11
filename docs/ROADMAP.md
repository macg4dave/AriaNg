# Roadmap

Goal: customize this fork for the owner's servers, improve speed and behavior
under larger loads, and provide easy deployment without storing private details.
This roadmap describes direction; individual tasks are tracked in [Backlog](BACKLOG.md).

| Stage | Intended outcome | Current state |
| --- | --- | --- |
| 1. Project guidance | Shared agent rules, reusable prompts, and project docs | Documentation established; implementation work remains separate |
| 2. Baseline | Core behavior inventory and reproducible synthetic performance workload | Planned |
| 3. Deployment | One-command deploy, dry-run, health checks, and rollback using external config | Local Debian/Nginx test container added; remote deployment is design only |
| 4. Targeted implementation | Optimize measured bottlenecks; introduce Go/Rust components where justified | Awaiting baseline and concrete component requirements |
| 5. Load validation | Demonstrate improvements and recovery under agreed workloads | Awaiting targets and implementation |

Baseline and deployment preparation may proceed independently where inputs are
known. Server implementation requires a concrete use case and an architecture
decision. Do not assume that both a Go and a Rust implementation are required.

Open planning inputs include expected concurrency, task counts, update frequency,
latency/resource budgets, hosting capabilities, and branding/customization scope.
Record generic requirements here; keep actual infrastructure details external.
