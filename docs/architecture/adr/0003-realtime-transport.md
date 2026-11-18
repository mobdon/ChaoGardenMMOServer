# ADR-0003 — Real-Time Transport

**Status:** Accepted

## Context
MVP gameplay is mostly single-player with async multiplayer, but care actions,
co-op visit reactions, and future live presence need push updates from the
server to the client.

## Decision
- **WebSockets (WSS)** via the NestJS gateway for gameplay intents and server
  events; **REST** remains available for everything (WS is an optimization,
  not a requirement) so the client works through restrictive networks.
- One socket per app process; events are **per-user ordered** using a
  monotonically increasing `seq` per user (persisted cursor in Postgres).
- Reconnect protocol: client sends `sync {sinceSeq}`; server replays missed
  events from the persisted outbox (`user_events` table, 24 h retention),
  then falls back to a full snapshot if the cursor is older.
- Heartbeat ping/pong every 25 s; server drops dead sockets after 2 misses.
- Redis pub/sub fans out events from API instances and the tick worker to the
  socket gateway owning the user's connection.
- Push notifications (FCM/APNs) are used only to wake the app for widget
  refresh and high-value events (evolution, competition results); never for
  gameplay data delivery.

## Consequences
- At-least-once delivery with client-side dedupe by `seq`.
- REST fallback guarantees feature parity; WS handlers thin-wrap REST services.
- Phase 2: dedicated gateway pods horizontally scaled behind the lobby;
  sticky sessions not required thanks to pub/sub fan-out.
