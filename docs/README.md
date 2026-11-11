# Project documentation

Read the root `AGENTS.md` for working rules, then use these documents as the
source of project context. Claude also reads the root `CLAUDE.md`.

| Document | Purpose |
| --- | --- |
| [Architecture](ARCHITECTURE.md) | Current system, boundaries, and architectural decisions |
| [Roadmap](ROADMAP.md) | Direction, sequencing, and unresolved decisions |
| [Backlog](BACKLOG.md) | Identified work, status, dependencies, and acceptance criteria |
| [Development](DEVELOPMENT.md) | Source layout, build commands, and verification |
| [Docker testing](DOCKER.md) | Debian/Nginx test image and local lifecycle commands |
| [Performance](PERFORMANCE.md) | Workload definition, measurement rules, and results |
| [Deployment](DEPLOYMENT.md) | Deployment design, external configuration, and rollback |

[Go](prompts/GO.md) and [Rust](prompts/RUST.md) prompts contain reusable language
guidance and instructions to discover these docs. Do not store project facts,
task inventories, architecture decisions, or implementation status in prompts.

Keep these documents current as work lands. Label proposals and unknowns clearly;
mark backlog items complete only when their acceptance criteria are verified.
Record significant architecture decisions in `ARCHITECTURE.md`, including their
reason and consequences. Documentation must never contain private server details.
