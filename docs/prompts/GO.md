# Go implementation prompt

Copy this prompt into your coding assistant and replace the task placeholder.
Project context belongs in the referenced documentation, not in this prompt.

```text
Implement the following task using Go:

TASK: <task description or backlog item ID>

First read the repository's agent instructions and documentation index. Follow
its references to architecture, roadmap, backlog, development, performance, and
deployment documentation. Inspect the relevant source before editing. If no index
exists, discover the corresponding documents without assuming a particular stack
or repository layout. Distinguish implemented behavior from proposed work.

Use those documents as the source of project requirements, compatibility rules,
configuration boundaries, verification commands, and acceptance criteria. Do not
copy repository context into this prompt or infer authorization for unrelated
backlog work. Resolve routine implementation choices within the requested scope;
ask only when a missing decision prevents safe or correct progress.

Use idiomatic Go and gofmt. Prefer small packages, explicit dependencies, and
interfaces at useful boundaries. Propagate context cancellation and deadlines;
bound goroutines, queues, payloads, and connection pools. Close resources and
support graceful shutdown. Handle shared state without data races.

Use the documented toolchain and module layout. Keep go.mod and go.sum consistent.
Run relevant formatting, go vet, and tests from each affected module. Use the race
detector where supported and relevant. For performance claims, use representative
benchmarks and profiling, including allocation measurements where useful.

Preserve required behavior and data, with a documented migration path where
needed. Validate meaningful behavior and integration boundaries. Use synthetic or
sanitized workloads for load testing, and compare measurements under equivalent
conditions. Test failure and overload recovery where relevant. Do not invent
performance results or treat language choice as proof of speed.

Keep secrets and private infrastructure out of tracked files, public artifacts,
and logs. Follow the documented deployment and authorization boundaries.
Implement and verify the requested work rather than stopping after a proposal.
Update the appropriate project documents when architecture, task status, commands,
or measured results change; keep this prompt reusable and repository-neutral.

Finish with the changes made, checks actually run, measured results where available,
and remaining limitations. Inspect the diff for private data and unrelated changes.
```
