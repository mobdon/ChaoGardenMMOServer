# Tiny Garden: verified server source, November 18, 2026

**Review target:** `codex/tiny-chao-shop`, from care base
`22add7030e2e8003a6ac05c202aff58864c9fcbf` through
`f689493eb38564dbece890eb0983ff6d415028f6`. The three cartridge-backed games
were bound to the real host at `d6306b33863d6a3101a4bfcf92a713823c4d5f2e`;
the final commit adds the Ball partial-payout test and supporting evidence.
Paths below refer to that branch; for example, `git show f689493:PHONE.md`.
The documentation checkout starts at `f0fa6a278b1d1ffc421924d75dbbccc323bfc5e9`.

| Server slice | Evidence and scope |
| --- | --- |
| Shop/residency - `dc55ae6`, `src/care.rs`, `src/watch.rs`, `TINY_SHOP.md` | Durable buy/feed and explicit Tiny resident opt-in. Seven A2NE fruit rows and prices require the verified local payload. Their effects on canonical needs and points use adapted rules; the three-ring Garden Fruit is also adapted. Browser QA at this checkpoint observed 20 -> 17 rings, one fruit purchased and consumed, and the resident routine suspending/resuming across a minigame. |
| Durable AFK training - `da6b2ac`, `src/training.rs`, `TRAINING.md` | One persisted job with a frozen policy and trusted host time. Five eligible stats, whole minutes, one point/minute, up to eight hours/480 points, free. Settlement updates only the selected canonical stat's points (maximum 9999), preserving its stored level and grade. Fourteen service tests plus three socket tests cover start/cancel/settle, competing intents, offline restart, save rollback, receipt replay and the post-commit completion event. This is an MMO policy, not a cartridge system. |
| Eggs/owned Chao - `d143111`, `src/collection.rs`, `TINY_EGGS.md` | Original A2NE weighted stock, twelve native prices and native newborn initialization; hourly offers, one held egg and eight total Chao are MMO rules. Purchase persists the egg; explicit online hatch adds a new owned record without changing the active Chao, owner, credential, gear or wallet beyond the purchase. The configured-source egg/fruit/transport run passed 25 cases, including two sockets, rollback and restart. No active hatchling selection, second playable Chao or proven jewel appearance mapping. |
| Shared source ports - `b3c5b40`, `9b4e959`, `7204263`, `crates/tiny-garden-core` | A2NE fruit growth/RNG, hatch initialization, shop and weed state machines have bounded Rust ports. The host uses the newborn initializer and relevant egg stock/price logic. Native toy ownership, weed interaction and full native growth on the canonical Chao do not follow from those core tests. |

## Five game IDs; three cartridge-backed host paths

The server selects the rules independently at startup. Omitting a private
payload leaves the game on `adapted-v1`; a configured invalid payload fails
startup. These are gameplay parity tests at the documented callback boundaries.

| Game | Rules and evidence at the review target |
| --- | --- |
| Memory | `a2ne-memory-v1` via `CHAO_TINY_GARDEN_PAYLOAD`; `a4a3c73`, `5f8fc5c`. Forty complete rounds, 79,755 original CPU frames. A real authenticated socket solved the six-column, five-row board from public observations. A failed file write withheld its positive award; retry and restart preserved exactly one credit and receipt. |
| Rock Paper Scissors | `asoe-rps-v1` via `CHAO_TINY_ASOE_PAYLOAD`; `8dda777`, `4adced1`. 7,279 comparisons including 32 complete original controller rounds. The socket test checks lane selection, expiry of held input and credit only after the cartridge result transition. The server supplies each lane's card. |
| Ball | `a3ve-ball-v1` via `CHAO_TINY_A3VE_PAYLOAD`; `e69a8c0`, `f689493`. 4,621 comparisons including five full native sessions. Seed 26 catches a ring on frame 854 and quits via the native menu on frame 877. The host replay gave 20 -> 21 rings once after repeated save and restart. Socket tests separately cover pause, input rejection and explicit cancellation. |
| Bounce | Adapted. A native A2NE round is pending verification and host integration at this checkpoint. |
| Search | Adapted. No verified original Search source is bound; the existing Search game is playable under its adapted rules. |

The configured native award limits are 99 (Memory/RPS) and 128 (Ball);
adapted titles retain 100, and the shared wallet cap is 999999. Memory and RPS
do not implement the original active pause menu. This evidence does not cover
complete native graphics/audio, BIOS hardware timing, Android installation or
wall-clock performance under load.

## Browser Stats tab QA

Client branch `codex/tiny-chao-phone` is based at
`de23ecd067bf24c3c489f85165f6252d17efc88a`. The native presentation source
is `ac7399b7b03dbc9e52fd4a7fe3dba965347ff483` from
`origin/codex/tiny-widget-native-data`; the managed source checkout is
`tiny-phone-source/mmo`. In isolated browser QA, Garden and Widget both
started with the Stats sidebar closed. Clicking Stats opened it; a second
click collapsed it. Both used the same native font and showed the read-only
Rust snapshot of the disposable Chao with 928 rings. The browser UI changes
were uncommitted at this checkpoint. This test did not install an Android APK
or exercise a launcher widget.

## Contract, verification and rollout boundary

- Resume the canonical credential with `POST /api/session` and `resumeOnly:true`.
  The existing authenticated WebSocket controls one profile. The widget's
  `observe=1` socket is read-only and does not hold a game open. Secure pairing,
  revocation and exclusive device leases remain a separate coordinated migration.
- Use the existing `care` envelope with a UUID and `expectedRevision` for
  `tiny_enter`, `tiny_purchase`, `consume`, `training_start`, `training_cancel`, `training_settle`, `tiny_egg_purchase` and `tiny_egg_hatch`.
  A durable `care-result` confirms a mutation. Private self snapshots expose
  `care.tinyShop`, `care.training`, `care.tinyEggShop` and `care.tinyCollection`.
- Read `/api/status` -> `tinyGarden.nativeTinyGames[gameId]` and each active
  `tinyGame.rulesVersion`. Existing `tiny-start` and `tiny-stop` are actions;
  native `tiny-input` carries only a session ID and `{kind:"native_buttons",held}`.
  The host owns frames, edges, randomness, result and wallet. Explicit stop
  cancels; the native Ball menu can bank a partial score. A completed result
  becomes `saved:true` only after the profile and matching receipt commit.
- Source hooks are in `src/{game,phone,service,native_tiny}.rs` and the reusable
  Rust crates. Verification used disposable stores, loopback WebSockets and
  private source fixtures: host tests, separate core/games tests, formatting,
  strict all-target Clippy, opt-in `tests/protocol.rs` native games, and the
  host's `native_ball_pause_quit_saves_exact_original_partial_award` test.
  Runtime boards are transient; committed results survive restart.
- The frozen live executable remains SHA-256
  `51A07F9C382ECB70785654297180349DD2E34F677D5921818EA2B9A6ECEC3100`.
  The source checkpoints and disposable checks did not deploy or replace it,
  migrate live saves, change credentials or authorize public exposure. Browser
  and Android adapters belong in `mobdon/ChaoGardenMMOClient`. An installed
  launcher widget, secure storage and background refresh still require device
  evidence; a web preview or debug APK alone does not prove them.
