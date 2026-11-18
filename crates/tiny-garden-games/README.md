# Tiny Garden games

Five functional, deterministic Rust game cores for the phone/playground and authoritative service. These are initial adaptations of the documented Tiny Chao Garden loops. They are not byte-for-byte GBA ports, extracted game engines or production-certified competitive games. The crate contains no original ROM, graphic, music or decoded native table.

This standalone crate does not modify the parent server manifest. The hosting service chooses its integration and persistence policy.

## API

```rust
use tiny_garden_games::{TinyGame, TinyInput, TinySession};

let mut round = TinySession::new(TinyGame::Memory, 123); // host-created seed
round.advance(20).unwrap(); // trusted host elapsed time only
let public_view = round.view(); // serializable, no hidden board/order/seed
let _ = round.input(TinyInput::MemoryFlip { cell: 3 });

if round.outcome().is_some() {
    let earned = round.claim_result().unwrap(); // local latch, once only
    // Commit earned.rings together with a durable unique-session receipt.
}
```

`TinyGame` wire IDs are `memory`, `rock_paper_scissors`, `bounce`, `ball`, `search`. `TinyView` serializes as `{ "game": "memory", "state": { ... } }`. Public fields and coordinates are defined in each module's `*View` structs. `TinyOutcome` contains the game, earned rings, completion flag, terminal reason and trusted elapsed time.

Private sessions do not implement serialization/deserialization. Their custom `Debug` prints only sanitized public state and outcome. Sessions implement `Clone` for host staging; a clone is **not** an independent reward identity. A host must use the same durable round ID when staging a cloned session.

| Input `kind` | Fields and bounds |
| --- | --- |
| `move` | `x`, `y`: signed integers −1, 0 or 1. Bounce/Ball accept horizontal movement with `y:0`. Search normalizes diagonal speed. |
| `memory_flip` | `cell`: 0–23; must contain a hidden unmatched card during `choose`. |
| `memory_trip` | Once during shuffle; revealing a card depends on the active push. |
| `rps_select` | `slot`: 0–2. |
| `rps_shoot` | No target ID; the selected available card becomes a simulated projectile. |
| `bounce_spring` | Start or press during a valid compression timing window. |
| `ball_shoot` | `color`: `red` or `green`. |
| `search_pick_up` | `chao`: a visible ID, validated within pickup distance. |
| `search_return` | Requires a carried Chao and proximity to the entrance. |
| `search_hint` | `orb`: 0–3, validated proximity and cooldown. |

All intents reject surplus fields, including forged score/time/results attached to otherwise valid unit actions. The parser exposes no score, success, collision, reward, seed or clock mutation intent. Input-rate restrictions are enforced where an input has an effect; a host should additionally bound messages per socket and discard excessive queued inputs.

## Implemented rules and evidence

The user-supplied [Chao Island guide](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/) is a first-hand mechanics guide, not original engine source. Read each game separately because distribution versions differ.

| Game | Complete implemented loop | Adaptation / unresolved constants |
| --- | --- | --- |
| [Memory](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/chao-memory.html) | Seven pairs/14 cards, initial peek, visible card pushes into blanks, one trip attempt, pair selection, three mistakes, position values 1/3/5 and full-clear bonuses 60/30/10 for zero/one/two mistakes. Earned rings survive failure. | 6×4 layout with blanks and zones, 2-second peek, five 300ms pushes, 600ms mismatch reveal and scoring each matched card by its zone are explicit local choices. Native layout, timings, push walk and mixed-zone payout need direct validation. |
| [Rock-Paper-Scissors](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/rock-paper-scissors.html) | 30-second timer; ten orbiting cards; three hand slots; five reserve replacements; authoritative shot hits; win awards one ring, tie clears without rings, loss/miss consumes a replacement. Clearing all targets redeals and adds ten seconds. Timeout or all cards exhausted ends. | A 100×100 field, circular orbit, shot speed/radius and acceleration are adapted. Reserve exhaustion does not discard still-usable hand cards. No client-declared target or hit. |
| [Bounce](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/chao-bounce.html) | Horizontal spring control, manual/automatic start, uncontrollable airborne Chao, ring contact, spring catch and timed stronger bounce, shorter head catch, three missed catches, four increasingly high levels, reflecting balloons in levels three/four and terminal full clear. | Physics, field dimensions, eight rings per level, drift, compression window, head location and balloon geometry are adapted. Coordinates use Y up. Ring total is this layout's count, not a claimed native maximum. |
| [Ball](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/chao-ball.html) | Move/shoot either color, projectile landing, sandwich flips, connected groups of at least ten clear, lower balls compact upward, rings freed from cleared supports fall and require a catch, descending-board loss, successive faster stages. | 8×12 board, orthogonal adjacency/flips, shot cadence, row generation, ring placement and 45-second/60-cleared stage transition are adapted. Coordinates use Y down in cell units. Surviving stages does not itself award rings. |
| [Search](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/chao-search.html) | 120 seconds, seven private targets, name requests, proximity hints, pickup/carry/entrance return, increasing reward schedule, ten seconds per correct return, thirty seconds lost per wrong return, all-found or timeout result. | Map, 16 candidates, movement, static activity poses, distance-based visibility and hint geometry are adapted. Current identity and future target order remain private. Wrong candidates return to their original map spot. |

The Search guide describes seven rewards increasing by five from five, yet also states a 105-ring maximum. Those claims conflict: `5+10+15+20+25+30+35 = 140`. The default follows the stated progression and flags unresolved native reward fidelity. `SearchConfig::with_rewards([u16;7])` lets the trusted host choose a later verified schedule; each entry is capped at 1,000. We do not claim that 105 or 140 is the verified native total.

The excerpted guide rules establish a loop; GBA native code/constant validation, original layouts/curves, original animation/audio and frame-accurate reproduction remain separate work. Memory deliberately reveals only the current legal peek/selection/trip; Search exposes visible candidate positions and clues without identifying which candidate is the requested target.

## Authority and integration gate

`advance` uses fixed 20ms steps and retains substep remainder. Each call is bounded to 1,000ms. Tick partitioning produces the same state for the same seed and intents. This is an API for a **trusted host**: a caller allowed to pass arbitrary elapsed time or inspect private memory is part of the authority boundary.

The host must bind one server-generated session ID to one authenticated player, select the seed, measure elapsed time, validate the active mode and accept only `TinyInput`. Never accept client elapsed time, seed, world position, board state, ring total, collision or outcome. Reject stale/other-player session IDs; isolate public views to their owner; suspend incompatible mission/race/mount/reward actions. Decide explicit restart/stop/disconnect policy and invalidate the old round. Movement intent should expire when input is stale or the connection ends.

`claim_result` refuses active rounds and a second local claim; terminal state rejects later inputs and freezes its result. This is not a database transaction. Before spendable rewards are enabled, atomically persist the account ring change **and** a unique round receipt. Retry after failure without minting a second award, and block replacement of an uncommitted finished round. Persisting a sanitized view cannot resume a hidden round; retain trusted server state or cancel it. RPS/Ball are survival loops, so the host may add a clearly labeled maximum session duration.

## Validation

From the workspace root, with the project-local Rust environment:

```powershell
. ./scripts/use-rust.ps1
cargo test --manifest-path server/crates/tiny-garden-games/Cargo.toml --offline
cargo clippy --manifest-path server/crates/tiny-garden-games/Cargo.toml --offline --all-targets -- -D warnings
```

Tests exercise all five loops and failure paths, real shot/ring geometry, color conversion and compaction, spring timing/balloons, Search pickup/return penalties and configurable rewards, Memory bonuses/mistakes, strict malformed-result rejection, hidden public data, deterministic tick partitioning and one-time immutable results. Some internal arrangement helpers put a game at an edge case; those are mechanics tests, not a claim that a human completed each whole game through a browser.

Host acceptance additionally requires two-user ownership isolation; restart/stale-ID rejection; no wallet credit before terminal; save-failure retry and durable receipt idempotence; replay/forged input tests; authenticated disconnect handling; and real touch/browser playability of all five renderers. Core tests alone do not certify those integrations.
