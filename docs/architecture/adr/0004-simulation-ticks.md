# ADR-0004 — Simulation Ticks

**Status:** Accepted

## Context
Needs decay over real time; evolution and competitions must be deterministic
and auditable, and identical on client and server.

## Decision
- All fractional gameplay math uses **fixed-point integers** (scale 1000) in
  `packages/simulation`; no floating point in authoritative paths.
- **Lazy catch-up ticks:** needs are computed on demand from `lastTickedAt`
  whenever state is read or mutated (correct for offline players), and a
  **background tick worker** runs every 60 s over `chao WHERE
  next_tick_at <= now()` to advance time-based events (evolution, sleep wake,
  egg hatch) and emit events.
- Ticks are idempotent: the worker bumps a per-chao `tickVersion` and uses it
  as an optimistic-concurrency guard; a lost race re-reads and re-applies.
- Evolution checks run inside the same transaction as the tick.
- Competition runs replay the same simulation module over frozen snapshots in
  a single worker process (no cross-shard nondeterminism).
- Simulation package version is recorded in tick metadata so a formula change
  is traceable; mid-life formula changes only apply to future elapsed time.

## Consequences
- No cron-style penalty for idle players; state is always correct on access.
- Worker crash ≤ 1 min of event latency, no data loss.
- Client prediction uses the identical module, so drift is bounded to
  in-flight intents.
