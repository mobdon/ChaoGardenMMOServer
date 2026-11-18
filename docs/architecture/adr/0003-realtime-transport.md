# ADR-0003 — Actual raw WebSocket transport

Status: existing prototype transport with coordinated migration gate,
November 18, 2026.

The service uses Axum HTTP and raw `/ws?token=...`, not a Socket.IO namespace.
Snapshots identify `selfId` and contain `players[]`, trusted `serverTime` and
supported activity/Tiny views. Clients submit bounded movement/action intents;
they do not upload canonical positions, balances or whole saves.

Current limits include allowed-Origin validation, 2 KiB WebSocket payloads,
80 messages per second per connection and input expiry. These are implemented
guards, not a measured capacity claim or a complete public-hosting design.

Explicit `saved`, durable Tiny result and Watch acknowledgements have distinct
meanings. Render snapshots for presentation; use the correct durable result
when confirming a save/reward. Reconnect must resume the same canonical profile.

Preserve this path while agreeing the proposed first-auth-frame, observation,
lease, sequence and operation envelope in
[PROTOCOL.md](../../coordination/PROTOCOL.md). Update all three clients and
negative/reconnect tests together. No absent REST endpoint or imagined
snapshot-version field becomes implemented by documenting it.

References: [service dispatch](../../../src/service.rs),
[snapshot projection](../../../src/game.rs), [real socket tests](../../../tests/protocol.rs),
[Tiny protocol](../../../PHONE.md), [Watch protocol](../../../WATCH.md).
