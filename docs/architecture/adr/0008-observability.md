# ADR-0008 — Evidence-driven health and capacity

Status: source limitations and required validation, November 18, 2026.

`/api/status` reports prototype state, assets and terrain fallback. Added
`/livez`, `/readyz` and `/metrics` source must be checked against the exact
candidate binary; their presence in current Git does not put them in the
earlier running preview.

Liveness only indicates a responding process. Current `readyz` calls
`create_dir_all`; success on an existing directory does not prove canonical
writes/fsync/rename succeed on a full/read-only volume. Do not use that result
as a durable-save guarantee.

The [metrics source](../../../src/metrics.rs) and
[review](../../coordination/VIBE-REVIEW-2026-11-18.md) record build and coverage
gaps. Event-drop instrumentation is not connected to all drop paths;
persistence errors are not counted consistently. Counters/alerts alone cannot
prove once-only rewards, a good backup or capacity.

Candidate evidence should include save latency/failures, tick delay, socket
backpressure/drop behavior, resident count, memory/CPU and validated workload
thresholds. Force timeout/missing-ack/storage/threshold failures to return
failure. Keep profile data and bearer query strings out of public metrics and
logs. Record which observations actually ran and which remain proposed.

References: [service probes/scheduler](../../../src/service.rs),
[durable-save/socket tests](../../../tests/protocol.rs),
[operations](../../release/operations/liveops-runbooks.md).
