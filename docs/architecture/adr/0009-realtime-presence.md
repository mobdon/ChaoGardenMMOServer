# Real-Time Presence and Garden Visits — Architecture Decision Record

Status: Accepted
Date: 2026-11-18

## Context

The Chao Garden MVP needs live multiplayer presence: seeing friends online, watching
visitors enter your garden, receiving reactions, and being notified about competitions
without polling. The transport is Socket.IO over the existing NestJS `/game` namespace,
with Redis used for fan-out across API instances. The REST surface already has
authorization, validation, rate-limiting, and persistence services; the WebSocket layer
must reuse them, not invent a parallel path.

## Decision

### Rooms

Two room families, both keyed by account id (never by email or raw client input):

- `user:<accountId>` — private room; notifications and counters are fanned out here.
- `garden:<ownerAccountId>` — garden room; the owner plus visitors join this room to
  receive presence, visit, reaction, decoration, and competition events.

Rooms are capped (`MAX_GARDEN_OCCUPANTS = 8`, `MAX_USER_ROOMS = 8`). Join attempts to a
ninth garden room are rejected, so a client cannot be tricked into amplifying fan-out.

### Event sequencing

Every emitted realtime event carries a monotonically increasing `seq` for its room.
The sequence number is stored in Redis (`seq:garden:<id>`) and incremented by the
emitter. Clients track `lastSeq`; on reconnect they send `sync` with the last seen
sequence. If events were missed, the gateway answers with a fresh snapshot plus the
current sequence, and the client resets its window. If the client's sequence is within
the retained event buffer, missed events are replayed (`recovered: true`).

Snapshots come from the same repositories used by REST (garden snapshot, notification
counters, competition status), so realtime and REST never disagree.

### Commands

Client → server messages are commands, not broadcasts. Each command handler:

1. resolves the authenticated actor (no anonymous commands),
2. validates the payload with hand-rolled validators from `@chaogarden/contracts`,
3. enforces per-connection and per-user rate limits via `RateLimitService`,
4. mutates state through domain services / repositories (PostgreSQL),
5. fans the approved event out to rooms with a fresh sequence number.

The gateway never relays client-authored text, creature stats, currency, or arbitrary
payloads. The only freeform-adjacent inputs are identifiers and enumerated reaction keys
from `REACTION_CATALOG`. Garden decoration changes are commands that persist first; the
emitted event contains only server-canonicalized fields, so visitors cannot spoof state.

### Presence

Presence is per-user (all of an account's sockets). `joinUserRoom` / presence counters
are tracked in Redis sets with a TTL refreshed by a heartbeat ping; heartbeat also
handles clean disconnects. `leaveAllRooms` on disconnect, and a `flushPresence` sweep on
process boot handles crashed processes.

### Degradation to polling

If Redis is unavailable or the socket cannot connect, the mobile client falls back to
polling `GET /garden/snapshot` on an interval. The contracts expose the same snapshot
shape for both paths, so the fallback is a transport swap, not a feature fork.

### Rate limits and bounds

- `RATE_GARDEN_EVENTS`: per-user command rate (default 30 / 10s).
- Reaction payloads are a single catalog key; decoration payloads are ids only.
- Maximum payload size enforced at the adapter level (default 64 KB).
- Room occupants and per-socket garden membership are bounded.

## Consequences

- All client messages flow through one guard → validate → rate-limit → persist → fan-out
  pipeline; there is no "raw relay" path.
- Reconnect is safe: seq + snapshot recovery means missed events are detected, not lost.
- Tests cover unauthorized joins, reconnects, duplicate commands, room isolation,
  Redis fan-out, and snapshot recovery.
