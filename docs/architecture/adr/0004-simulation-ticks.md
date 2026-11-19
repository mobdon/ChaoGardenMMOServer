# ADR-0004 — Trusted clocks and simulation

Status: existing adapted host scheduling; durable-job integration proposed,
November 18, 2026.

The Rust host advances `Game` in 50 ms steps and schedules public snapshots at
10 Hz. Missed ticks use the configured skip behavior; simulation time does not
necessarily equal wall time under load. Tiny cores retain their own fixed-step
remainder and are advanced by the host, not a client elapsed-time claim.

Native timings, animation playback and MMO tuning are separate evidence.
Browser rendering may interpolate or select motion from snapshots. It cannot
award progression by running a local clock or claim its predicted result.

Watch performs no downtime catchup and grants no passive stats/rings. A server
restart restores a fresh resident routine. Unfinished Tiny boards do not resume
as serialized hidden game state; see the explicit policies in
[PHONE.md](../../../PHONE.md) and [WATCH.md](../../../WATCH.md).

Future AFK training must use trusted persisted timestamps, bounded policy and
once-only settlement. The standalone ledger is not a durable running service.
Test equivalent time partitions, stale/replayed intents, clock boundaries,
restart and failed saves before enabling rewards.

References: [host scheduler](../../../src/service.rs),
[Tiny games](../../../crates/tiny-garden-games/README.md),
[training core](../../../crates/tiny-garden-core/README.md).
