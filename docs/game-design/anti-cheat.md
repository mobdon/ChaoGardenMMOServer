# Server authority and validation requirements

Updated November 18, 2026. These requirements apply to the Rust authority;
they do not accept the generated TypeScript simulation/API as a replacement.

## Existing boundary

Clients submit intents. Rust owns position/terrain, clocks, active modes,
progression, rings, shop prices, combat/race validation and saves. Forged
coordinates, admin/stat/reward fields cannot grant the corresponding state.
Role-sensitive gear/mech actions use the stored owner role. Native cosmetic
attachments do not grant unimplemented character upgrades.

Tiny input binds to the current server-created round ID and rejects surplus
result/time/score fields. Only public views are sent; seeds, hidden boards and
future Search order remain private. Terminal results project a bounded credit
and receipt into the profile transaction; no spendable credit/success event
before successful persistence. Watch care validates the current resident ID
and strict action shape; idle wandering grants no rewards.

## Integration gates

- Central Rust ownership/revocation/lease checks must cover every mutation,
  including separate Save/Watch handlers. Current shared bearer sockets are
  not exclusive device ownership.
- Durable requests need agreed operation IDs/revisions and same-result retry
  semantics. Input cadence/lease epochs are separate from durable revisions.
- AFK jobs and away competitions require trusted time, canonical activity
  exclusion, cost/receipt atomicity and restart-safe once-only settlement.
- Cached/offline UI may display observations; it may not upload claimed gains.
- Transport/body/rate bounds supplement semantic validation and cannot replace
  actual state/receipt checks.

## Evidence

Relevant existing cases are in [game tests](../../tests/game.rs),
[Tiny tests](../../tests/phone.rs), [ownership tests](../../tests/phone_review.rs),
[Watch tests](../../tests/watch.rs), [activity tests](../../tests/activities.rs)
and [real socket/save tests](../../tests/protocol.rs). Run them against the
reviewed candidate and isolated data; file existence is not a passing result.
Negative controls must fail on missing acknowledgements, duplicate payouts,
invalid auth and unverified workload thresholds.

Schema/lease details remain in [PROTOCOL.md](../coordination/PROTOCOL.md).
This is not a claim that all future anti-cheat surfaces are implemented.
