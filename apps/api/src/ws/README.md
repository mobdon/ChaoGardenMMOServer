# Real-Time Presence and Garden Visits (WebSocket layer)

Adds authenticated Socket.IO realtime to the NestJS API:

- `packages/contracts/src/realtime.ts` — event maps, command envelope with
  monotonic `seq`, bounded reaction set, payload limits, room helpers.
- `apps/api/src/ws/realtime.service.ts` — presence registry (Redis-backed with
  in-memory fallback), monotonic sequence per room, snapshot recovery.
- `apps/api/src/ws/garden.command.service.ts` — single authorization +
  validation + rate-limit + persistence path shared with REST. Never trusts
  client-authored stats/currency/text; decoration updates only accept catalog
  ids already owned.
- `apps/api/src/ws/game.gateway.ts` — handshake auth (via guard pipeline),
  per-user and per-garden rooms, heartbeat, room-size and message-rate caps,
  graceful presence cleanup on disconnect/crash (Redis TTLs).
- `apps/api/src/ws/redis-io-adapter.ts` — optional Redis adapter fan-out;
  degrades to single-node in-memory adapter when `REDIS_URL` is unset.
- `apps/api/src/ws/presence.service.ts` — presence TTL sweeper for process
  failure recovery.
- `apps/api/src/ws/realtime.module.ts` — wiring, registered in `AppModule`.

Clients that cannot hold a socket keep polling `GET /api/snapshot`; the
snapshot carries the room's current `seq` so reconnecting clients detect gaps
(`lastSeq` in `garden:join`) and refetch when events were missed.

Tests: `apps/api/test/ws/*.spec.ts` cover unauthorized joins, reconnect
sequencing, duplicate commands, room isolation, Redis fan-out, and snapshot
fallback recovery. Integration tests require a running Redis/postgres and skip
otherwise (`SKIP_INTEGRATION`).
