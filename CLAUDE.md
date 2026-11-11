# Claude instructions

Prefer Rust over Python: if it can be done in Rust, do it in Rust. Apply this to
test automation, development/deployment tooling, and scripts as well as application
code. Follow the shared rule in `AGENTS.md`; keep existing Python tooling until
migrated and use Rust for new implementations and replacements. Go remains an
approved choice for application server components.

Read and follow [AGENTS.md](AGENTS.md) before working in this repository. It is the
shared source of project guidance for Codex and Claude, including architecture,
commands, customization conventions, secret handling, and deployment constraints.

Read [docs/README.md](docs/README.md) and follow its links for project context,
including architecture, roadmap, backlog, development, performance, and deployment.
Keep repository facts in those documents and language prompts repository-neutral.
Keep shared rules in `AGENTS.md` so the two assistants do not drift apart.

Store any machine-specific Claude instructions in ignored `CLAUDE.local.md` and
private settings in ignored `.claude/settings.local.json`. Do not include real
server details or credentials in tracked Claude configuration.
