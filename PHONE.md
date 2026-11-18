# Tiny Garden phone/playground protocol

The primary Rust authority hosts five playable Tiny Garden minigame
adaptations: `memory`, `rock_paper_scissors`, `bounce`, `ball`, and `search`.
They use the same canonical Chao profile and bearer credential as the main
browser game. They do not execute an original GBA ROM or claim native timing,
graphics, physics or complete engine fidelity. Core rules and source/evidence
limits are documented in `crates/tiny-garden-games/README.md`.

The Node implementation remains an older gameplay reference and does not host
these minigames. The Rust `/api/status` response advertises
`tinyGarden.games`, `rules:"adapted"`, and `maxRingReward:100`.

## Credentials and session identity

Resume a selected browser save with `POST /api/session` and JSON
`{ "token":"...", "resumeOnly":true }`. Unknown/missing credentials return
404 without creating another Chao. Connect `/ws?token=...` as in the main game.
Separate-device pairing and an exclusive device lease are not implemented.
Multiple sockets using one credential share one profile, one clock, one active
minigame and one reward receipt. They can issue competing intents.

Start a game:

```json
{"type":"action","action":"tiny-start","gameId":"memory"}
```

Starting from a garden/hub canonically returns the Chao home and dismounts an
existing owner vehicle. Active mission/race/Karate entry cannot be bypassed.
Only one minigame may run per profile. Main movement and other gameplay actions
are blocked while it is active; explicit Save remains available.

Snapshots include:

```json
{
  "tinyGame": {
    "id":"server-generated-uuid",
    "gameId":"memory",
    "phase":"playing",
    "startedAt":123456,
    "view":{"game":"memory","state":{}},
    "result":null
  },
  "tinyResult":null
}
```

`state` contains each core's public `*View` fields, in snake_case. Hidden board
faces, future Search target order and PRNG seeds are not serialized. The self
player retains seven canonical stats and saved `tinyRecords` in snapshots.

## Input and exit

Send only an input intent, bound to the current server session ID:

```json
{"type":"tiny-input","sessionId":"server-generated-uuid","input":{"kind":"memory_flip","cell":3}}
```

The core README lists all input kinds and bounds. Surplus fields inside an
intent are rejected, including forged score/damage/time/result claims. Move
input is bounded to integers -1..1, throttled to 25 Hz and released after 350 ms
without another movement intent. Other inputs are throttled to 12.5 Hz, with
additional core timing/collision/proximity rules. The host alone advances time.
Timers use trusted server simulation time in 50 ms ticks. Skipped ticks under
load stretch their wall-clock duration; native timing fidelity and load-tested
wall-clock behavior are separate acceptance work.

Stop using the same ID:

```json
{"type":"action","action":"tiny-stop","sessionId":"server-generated-uuid"}
```

Stale IDs cannot control or stop a later game. Stopping an unfinished game or
the last socket disconnect cancels it without payment. Closing one of multiple
sockets does not cancel another socket's game. A finished result awaiting disk
save remains pending through disconnect and cannot be discarded or replaced by
another game. After a successful result, another `tiny-start` can replace the
finished screen directly.

## Atomic earned rewards

Native-guide score schedules and MMO currency limits are separate. The service
caps one terminal game's credited reward at 100 rings and caps the wallet at
999999. `earnedRings` preserves the core's earned amount before those caps. The
100-ring cap is an MMO adaptation, including when Search's unresolved default
schedule totals 140 or an endless game earns more.

A terminal core result enters phase `saving`. Its proposed ring credit and
unique receipt are projected into the same profile JSON transaction. The live
wallet, live saved records and success event remain unchanged until fsync and
atomic rename succeed. Disk failure retains the pending result, blocks replay
or another round and retries; explicit Save can retry immediately. No client
result/score submission can trigger a payout.

Only a durable commit emits:

```json
{
  "type":"event",
  "kind":"tiny-game-result",
  "id":"server-generated-uuid",
  "gameId":"memory",
  "rings":86,
  "earnedRings":86,
  "reason":"all_pairs",
  "completed":true,
  "elapsedMs":5000,
  "saved":true
}
```

The same result appears in `tinyResult` and the finished `tinyGame.result`.
Saved `tinyRecords[gameId]` contains `plays`, `completions`, `ringsEarned` and
`lastResultId`. Ring credit and receipt survive restart together. Runtime board
state does not resume after a server restart. Receipts and the core claim latch
prevent repeat payment for the same server run.

Validation covers all five authenticated WebSocket start/stop flows, forged and
stale inputs, shared-profile clocks, activity exclusion, canonical dismount,
positive Memory earnings using public observations, failed/retried disk saves,
single receipt/credit, pending disconnects and loaded save compatibility.
