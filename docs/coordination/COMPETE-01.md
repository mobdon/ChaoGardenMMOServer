# COMPETE-01 implementation handoff

Initial base: published server `main`, commit
`282f6abd1bdb75d0477622f8952667073dc2b751`. Branch: `codex/ai-competitions`.
Implementation commit: `7c61e0a`. Rebased onto published `main` at `4056fcf`;
the later base commits changed documentation/mobile adapters, not Rust source.
Status: isolated Rust implementation and local proof; integration review remains
required. No deployment, live profile edits, or preview restart was performed.

The initial base did not contain the integration chat's newer coordination
board/protocol documents. Those were read from its separate checkout and are
present after rebase. Only this workstream's board row was updated.

The versioned implemented protocol and complete rules are in
[COMPETITIONS.md](../../COMPETITIONS.md). New owned modules are
`src/competition.rs`, `src/competition_sim.rs`; new host/protocol tests are
`tests/competition.rs`, `tests/competition_protocol.rs`, and
`tests/reference/competition-compatibility.test.js`.

Shared integration hooks are confined to `src/game.rs`, `src/service.rs` and
`src/lib.rs`: one competition ledger per canonical profile, exact input intents,
central activity exclusions, server ticks, profile projection/commit, save
acknowledgments, compact public progress and durable result summaries. Unknown
profile fields and foreign activity leases survive saves. `game.js` refuses
competition/lease profiles so the retained Node reference cannot erase them.

The published base could not compile: `service.rs` imported `Instant` twice and
initialized an undeclared Config field; `metrics.rs` held two mutable closure
borrows of one string. Minimal repairs are included. `main.rs` has only the
existing println formatted for the repository-wide formatting check.

## Integration contract

- One to three explicitly requested matches; no automatic budget renewal.
- One abstract race using `beginner-0-1` plus native rival rows 1–3, and one
  Karate bout against `native-karate-opponent-2`. No further courses or tiers.
- A plan owns `activityLease:{kind:"competition",id:planId}` until cancellation
  or final durable result. Training/account hosts must refuse that reservation
  and release/cancel it atomically before replacing the lease. Runtime foreign
  lease conflicts pause the plan and fail persistence instead of reclaiming it.
- The proposed accounts v2 envelope/device lease is not implemented here. When
  integrating it, apply its shared authorization guard to competition start,
  stop, replay and explicit Save before dispatching the strict inner intent.
- Competition revisions change on committed transitions, not each simulation
  tick. They supplement the future durable profile revision; do not reinterpret
  them as a per-device input sequence.
- Completed results, frozen replay, ring credit and receipt share the existing
  single-authority JSON transaction. No independent claim endpoint is needed.
- Five-second saved progress checkpoints resume without server-downtime
  catch-up. New entries and terminal results wait for durable commit.
- Native models/data stay ignored. Current native course availability and
  manual activity records/rewards are unchanged by the abstract simulation.

## Evidence and remaining limits

Validated November 18, 2026: 102 server tests, 22 Tiny game tests, 21 Tiny core
tests and 49 Node reference tests passed. The explicitly selected ignored local
catalog proof also passed. Repository/Tiny crate formatting and Clippy with
warnings denied passed; the optimized Rust server release build passed.
The separate ROM-dependent Tiny core proof was not invoked.

The proof suite covers both match types, stat scaling, placements, duplicate
devices/intents, foreign/training lease conflicts, failed/retried writes,
restart before/after results, cancellation, wallet caps, corrupted replay saves,
spectator replay and passive resident isolation. A real loopback WebSocket test
drives the authoritative start/disconnect/reconnect/replay protocol.

The explicitly invoked local-reference test reads only the two extracted
catalogs into a temporary authority. One race placed first and durably credited
25 rings; one Karate bout placed first and credited 50. Both saved replays and
the same canonical Chao survived restart. The native race stayed unavailable.
The fixture profile is an authorized synthetic owner with level-99 stats;
separate low/high-stat tests demonstrate scaling. This is proof of the two
adapted server slices, not a full native engine or graphical playtest.

Native path/obstacle fidelity, all remaining courses/tournaments, client replay
presentation, shared accounts-v2 authorization, durable training integration
and measured public-server capacity remain separate workstreams. Final PR
validation commands/results are recorded in its description.
