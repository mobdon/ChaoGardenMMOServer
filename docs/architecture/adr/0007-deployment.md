# ADR-0007 — Single authority and held rollout

Status: deployment gate/proposal, November 18, 2026.

The current Rust authority owns in-memory gameplay and local JSON profiles.
Keep one writer with an identified durable data directory and verified
backup/restore. Adding PostgreSQL/Redis services or multiple machines does not
migrate this state or provide distributed ownership.

Do not roll source into the playable preview as a side effect of merge/review.
At `0226dc9`, recorded compilation, migration, backup, readiness and deployment
script findings hold rollout. The separate tested preview binary and canonical
save remain independent of this documentation change.

Before a candidate deployment:

1. Pass Rust build/format/Clippy, host tests and both independent Tiny suites.
2. Prove legacy identity, rings, gear, Tiny receipts and resident continuity
   through quiesced copied-data restart/restore and deliberate save failures.
3. Coordinate any protocol migration with desktop, Tiny and widget consumers.
4. Measure real authenticated 100–500-player workloads and threshold failures;
   account for residents, tick time, snapshot fanout and persistence cost.
5. Record candidate/source/runtime hashes, rollback compatibility and explicit
   rollout authorization. Image rollback alone is not data rollback.

Future shared storage/sharding must define ownership, atomicity and recovery
before replicas write canonical state. No public hosting, database migration
or provider execution is authorized by this record.

References: [review blockers](../../coordination/VIBE-REVIEW-2026-11-18.md),
[storage source](../../../src/service.rs),
[operating procedure](../../release/operations/liveops-runbooks.md).
