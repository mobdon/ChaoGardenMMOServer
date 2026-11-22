# Tiny Garden phone/playground protocol

The Rust authority hosts five games using the canonical Chao and credential:
`memory`, `rock_paper_scissors`, `bounce`, `ball`, and `search`. On startup
the following private decoded source files select cartridge-backed Rust rules.
Explicitly configured missing, invalid or mismatched files fail startup. No
file is distributed. Unconfigured games use the existing adapted rules in
`crates/tiny-garden-games`. The retained Node reference does not host them.

| Game | Server configuration | Run rulesVersion | Maximum credited rings |
| --- | --- | --- | ---: |
| Memory | `CHAO_TINY_GARDEN_PAYLOAD` (A2NE) | `a2ne-memory-v1` | 99 |
| Rock Paper Scissors | `CHAO_TINY_ASOE_PAYLOAD` (ASOE) | `asoe-rps-v1` | 99 |
| Ball | `CHAO_TINY_A3VE_PAYLOAD` (A3VE) | `a3ve-ball-v1` | 128 |
| Unconfigured games, Bounce and Search | no verified full round bound | `adapted-v1` | 100 |

`GET /api/status` lists the available IDs in `tinyGarden.games` and reports
`tinyGarden.nativeTinyGames[gameId]={native,rulesVersion,maxRingReward}`. The
aggregate `rules` field is `per-game` when any native title is selected and
`adapted` otherwise; the aggregate `maxRingReward` is the largest per-game
limit. Choose controls using the **active run's** `rulesVersion`. Source-backed
rules cover the playable round; they do not imply native browser assets,
authentic BIOS execution or an embedded ROM emulator. Per-game CPU parity
evidence and exclusions are documented in `crates/tiny-garden-core/NATIVE_*.md`.

## Credentials and session identity

Resume a selected browser save with `POST /api/session` and JSON
`{ "token":"...", "resumeOnly":true }`. Unknown/missing credentials return
404 without creating another Chao. Connect `/ws?token=...` as in the main game.
A Tiny Garden socket (`/ws?tiny=1`) needs a one-time pairing with the PC:

1. PC: `POST /api/pair/start` with `Authorization: Bearer <Chao token>` returns
   `{ "code":"ABC234", "expiresInSeconds":600 }` (single use, one open code per Chao).
2. Phone, signed in to the same memory card: `POST /api/pair/claim` with JSON
   `{ "card":"...", "code":"ABC234" }` returns `{ "deviceKey":"...", "chao":{token, player} }`.
3. Phone: connect `/ws?tiny=1&token=...&device=<deviceKey>`. Without a paired
   key the handshake is refused with 403. One phone per Chao; pairing again
   replaces it. `tiny-start`, `tiny-stop` and `tiny-input` on a socket without
   `tiny=1` get a `tiny-needs-phone` notice.

Only key hashes are stored (`pairings.json`, mode 0600). An exclusive device
lease is not implemented.
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
    "rulesVersion":"adapted-v1",
    "startedAt":123456,
    "view":{"game":"memory","state":{}},
    "result":null
  },
  "tinyResult":null
}
```

Adapted `state` uses the existing snake_case public fields. Native states use
camelCase. Memory sends a 6-column × 5-row array of cells (only visible faces),
a cursor and the current walker and selected cards. Ball sends a 10×10 board,
1-based stage, screen, scroll, player, current shot/resolution and visible
falling rings in pixels. RPS sends its clock, visible targets, three current
card states, lane, marker, shots and actor; hidden targets and unrevealed
replacement gestures are excluded. Hidden board
faces, future Search target order and PRNG seeds are not serialized. The self
player retains seven canonical stats and saved `tinyRecords` in snapshots.

## Input and exit

For native rules send the current button mask. Edges, elapsed time and score
belong to the authority. The host accepts up to eight queued transitions,
deduplicates unchanged masks and consumes one per native frame. A release
wins if the queue is full. For a held key, refresh its mask more frequently
than 350 ms. Send zero on release, blur or navigation. Any disconnect clears
held input even if another connection keeps the round alive.

```json
{"type":"tiny-input","sessionId":"server-generated-uuid","input":{"kind":"native_buttons","held":16}}
```

A=1, B=2, Start=8, Right=16, Left=32, Up=64, Down=128. Ball accepts these
bits (including native pause/quit); Memory accepts all except Start; RPS
accepts only A/Left/Right. A dismisses native result screens. The active
pause menu is not ported for Memory/RPS. RPS fires the current lane's
source-generated card; the client cannot select a gesture. Unsupported bits,
additional fields and adapted intents are rejected for a native run. If a
native core rejects an unrepresentable cartridge state, the host shows
`phase:"error"`; cancel the round to start again. It yields no payout.

A host frame accumulator uses the GBA cadence of 16,777,216 clock cycles per
second and 280,896 per frame, driven by the existing 50 ms simulation ticks.
Skipped service ticks therefore stretch elapsed wall time. The browser sends
no frame counter, seed, time, finish or reward.

For adapted rules, send an input intent bound to the same server session ID:

```json
{"type":"tiny-input","sessionId":"server-generated-uuid","input":{"kind":"memory_flip","cell":3}}
```

The adapted core README lists its input kinds and bounds. Surplus fields inside an
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

The server uses the per-game limits above and caps the canonical wallet at
999999. Native Ball can retain caught rings on loss or cartridge pause/quit.
Using the protocol’s explicit tiny-stop always cancels an unearned round. `earnedRings` preserves the core's earned amount before those caps. The adapted
100-ring cap is an MMO rule, including when Search's unresolved default
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
  "rulesVersion":"adapted-v1",
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
