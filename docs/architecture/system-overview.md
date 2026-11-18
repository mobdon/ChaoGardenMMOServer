# System overview and state ownership

Updated November 18, 2026. Source reference: server `0226dc9`; recorded review
limitations remain in force. This is a source map, not a deployed topology.

## Existing path

Browser garden / Tiny Garden adapter in the separate client repository
→ authenticated HTTP session and raw WebSocket intents
→ Rust `service.rs` dispatch under the authoritative game mutex
→ Rust `Game`, Tiny games, Watch and activities
→ sanitized snapshots/events and the local profile transaction.

| Module | Actual responsibility |
| --- | --- |
| [service.rs](../../src/service.rs) | Configuration, routes, sockets, dispatch, simulation scheduling and `AppState::persist` |
| [game.rs](../../src/game.rs) | Canonical players, terrain movement, bounded actions, economy, public snapshots and profile projection |
| [phone.rs](../../src/phone.rs) | Private Tiny sessions, intent validation and pending/durable reward projection |
| [watch.rs](../../src/watch.rs) | Opt-in resident idle/wander/rest and bounded explicit care |
| [activities.rs](../../src/activities.rs) | Adapted validated race/Karate authority and source catalogs |
| [tiny-garden-games](../../crates/tiny-garden-games/README.md) | Five deterministic host-authoritative game adaptations |
| [tiny-garden-core](../../crates/tiny-garden-core/README.md) | Bounded native inspection and standalone training ledger; not a production job service |

## Persistence

`CHAO_DATA_DIR/profiles.json` is the canonical raw profile array. Terrain,
activity catalogs and imported assets also resolve under that data directory.
The service does not use the generated PostgreSQL/Redis model for gameplay.
One writer owns this directory; the mutex is not a cross-process storage lock.

`AppState::persist` exports profiles to `profiles.json.tmp`, writes and syncs
the temporary file, renames it over the canonical file, clears dirty state and
then commits pending Tiny rewards. A failed write must not become a success
acknowledgement or a spendable Tiny payout. Watch mutations stage rollback on
save failure. These are current boundaries, not a universal transaction ledger
for future features or a guarantee of every power-loss/filesystem scenario.

`Player::profile` projects known fields; arbitrary unknown fields are not
guaranteed to survive load/save. Versioned migration must prove preservation
before new account/job fields are enabled. The existing release-migration
rewrite is a reviewed blocker, not the normal atomic save path.

## Actual wire boundary

The current session endpoint returns `{token, player}`. Raw `/ws?token=...`
receives snapshots containing `type`, `selfId`, `serverTime` and `players[]`,
plus current world/activity/Tiny data. Select the canonical self by `selfId`;
do not assume an invented top-level `creature` or REST care/snapshot API.
Current clients share a bearer credential and can issue competing intents.
The [v2 accounts/observation/lease contract](../coordination/PROTOCOL.md) is a
proposal requiring coordinated garden, Tiny and widget migration.

See [PHONE.md](../../PHONE.md), [WATCH.md](../../WATCH.md) and
[README.md](../../README.md) for supported commands and durability events.
Snapshots alone do not prove a durable save.

## Planned extension gates

Durable AFK jobs, away-time competitors and device ownership must join the
same canonical profile/reward boundary. Android adapters belong in the client
repository and need a real service round trip plus device evidence. The
100–500 player target needs measured tick, socket, resident and persistence
costs before deployment sizing or replication. Generated infrastructure does
not make these modules distributed.
