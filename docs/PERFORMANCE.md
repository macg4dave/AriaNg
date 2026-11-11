# Performance

## Status

Speed and larger loads are project goals. No reproducible baseline, numeric
capacity target, or measured optimization result has been established yet.

## Measurement contract

Before optimizing, define synthetic or sanitized workloads covering relevant
concurrent users, RPC connections, task counts, update frequency, payload sizes,
and slow clients. Agree latency, throughput, error, and resource budgets where
possible; label provisional assumptions. Never store real server inventories.

Compare before and after using equivalent workload and environment conditions.
For servers, capture throughput, p95/p99 latency, errors, CPU, and memory where
relevant. For the frontend, measure responsiveness, large-list rendering, and
network/RPC use. Use release builds for compiled-server performance measurements.

Exercise disconnects, upstream failures, saturation, and recovery. Check bounded
concurrency, queues, payloads, retries, and memory growth. Caching and batching
must preserve freshness, correctness, and isolation. Profile before adding pooling,
complex concurrency, or low-level optimizations.

## Results record

When measurements exist, record the task ID, revision, exact commands, sanitized
environment description, workload, duration/repetitions, before/after results,
and limitations here or in linked reports. Do not infer capacity from language
choice or present unmeasured improvements as facts.
