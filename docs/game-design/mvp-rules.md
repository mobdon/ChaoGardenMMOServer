# Chao Garden implemented scope and next slices

Updated November 18, 2026. This records current prototype adaptations and
remaining user scope. It supersedes PR #9's replacement species/currency rules.
It is not a full native-game port or proof of public release readiness.

## Current Rust prototype

| Area | Current behavior / evidence boundary |
| --- | --- |
| Canonical Chao | Resume the same saved profile, owner role, seven stats, rings, alignment, cosmetics, gear and records |
| Stats | `swim`, `fly`, `run`, `power`, `stamina`, `luck`, `intelligence`; current prototype levels capped at 99 and points at 9999 |
| World | Hub and Neutral/Hero/Dark gardens; terrain/collision and alignment gates are authoritative |
| Care/training | Existing bounded location/cooldown/energy actions and fixed-price shop; deeper native animal/life mechanics remain separate |
| Activities | Adapted cooperative robot mission, ordered prototype race and native-catalog Karate authority |
| Native races | All 62 extracted configurations remain unavailable until real geometry/gates/terrain are verified |
| Tiny Garden | Five Rust adaptations: Memory, Rock-Paper-Scissors, Bounce, Ball and Search; same canonical profile and bounded earned-ring receipts |
| Residency | Explicit Watch opt-in; visible idle/wander/rest while the service runs, bounded pet/rest/wake; no passive skill/ring gains or downtime catchup |
| Owner extras | Authorized native Air Shoes/Hammer Gloves attachments and adapted Dark Walker controls; not full stage-character mechanics |

Exact current constants/actions are in [game.rs](../../src/game.rs) and
[tests/game.rs](../../tests/game.rs). Changes to tuning require source/tests
and a recorded native-rule-versus-MMO-adaptation distinction. Do not replace
rings with generated alternative currencies or reset the existing Chao.

## Planned feature slices

Tiny Garden is the phone app scope and also remains playable on desktop.
Connect durable AFK training through the existing Rust ledger only after
identity/activity authority and atomic job/profile persistence are agreed.
The Android widget is viewing/quick care; full games/training open Tiny Garden.

Away-time AI races/Karate need separate opt-in, eligibility, scheduling,
server-owned match outcomes and durable results. Watch wandering is not that
competition system. Preserve stage/mech/animal/toy/kindergarten/Black Market
ideas in the client design/task ledger; implement small verified slices rather
than presenting the catalog as completed mechanics.

## Completion evidence

Core tests, real protocol round trips, restart/save-failure tests and actual
browser/device behavior establish different claims. A Rust parser or converted
model does not prove an engine port; a UI timer does not prove AFK training;
a web preview does not prove an installed widget. Public capacity targets need
measured evidence.

References: [PHONE.md](../../PHONE.md), [WATCH.md](../../WATCH.md),
[ACTIVITIES.md](../../ACTIVITIES.md), [MECH_MOVES.md](../../MECH_MOVES.md),
[workstreams](../coordination/WORKSTREAMS.md), [architecture](../architecture/README.md).
