# System Overview & Data Flow

## Components

```
┌───────────────┐   HTTPS/REST    ┌──────────────────┐
│ React Native  │◄───────────────►│  NestJS API      │
│ mobile client │   WSS (realtime)│  (auth, gameplay)│
│  + native     │◄───────────────►│  + WS Gateway    │
│    widgets    │   push (FCM/APNs)└───┬────────┬─────┘
└───────────────┘                      │        │
                                   ┌───▼───┐ ┌──▼──────────┐
                                   │ Redis │ │ PostgreSQL  │
                                   └───────┘ └─(authoritative)
                                   ┌──────────────┐
                                   │ Object store │  (original art/audio/
                                   │  (S3-compat) │   decorations, CDN)
                                   └──────────────┘
                                   ┌──────────────┐
                                   │ Tick worker  │  (needs decay, evolution,
                                   │ (queue cons.)│   competition runs)
                                   └──────────────┘
```

## State ownership

| State | Owner | Client behavior |
| --- | --- | --- |
| Chao needs/stats/stage | Postgres (API + tick worker write) | predicts decay locally, snaps to server values |
| Inventory & currencies | Postgres ledger | displays server-confirmed balances |
| Garden layout | Postgres | optimistic placement, rejected placements revert |
| Presence / visit freshness | Redis (TTL snapshots) | read-only cache |
| Cooldowns / rate limits | Redis token buckets | disables buttons optimistically |
| Media assets | Object storage + CDN | immutable, content-hashed URLs |
| Widget data | Shared app-group store (native) written by the RN app after each sync | widget reads local store only; never networked |

## Data flows

1. **Care action (feed/train/clean/play/pet):** client → WS `intent` (or REST
   fallback) → Zod validate → semantic validate → Redis rate limit →
   Postgres transaction (state delta + ledger + idempotency record) →
   `state.delta` event broadcast to the owner's sockets.
2. **Offline catch-up:** on reconnect, client sends `sync {sinceVersion}`; API
   computes decay from `lastTickedAt` via the shared simulation, returns
   snapshot + events after `sinceVersion`; widget store updated.
3. **Simulation tick:** tick worker consumes a Redis stream of due chao
   (or batch-scans `chao WHERE next_tick_at <= now()`), applies decay/
   evolution, bumps `version`, writes `next_tick_at`; emits evolution events.
4. **Visit:** client → `GET /gardens/:userId` → API reads Redis snapshot
   (rebuild from Postgres on miss) → returns snapshot; reaction appended with
   rate-limit check.
5. **Competition:** enrollment close → worker freezes participant snapshots →
   deterministic run → results + rewards written in one transaction →
   per-season leaderboard in Postgres, cached in Redis.
6. **Widget refresh:** RN app writes latest snapshot to an app-group storage
   file; native widget timeline reads it. Server push (FCM/APNs) wakes the app
   on notable events (evolution, competition result) so the widget refreshes.

## Realtime events (schema, `packages/contracts`)

```jsonc
// envelope
{ "v": 1, "type": "state.delta", "seq": 42, "ts": "2025-01-01T00:00:00Z",
  "payload": { /* type-specific, Zod-validated */ } }
```

| Event | Payload (abridged) |
| --- | --- |
| `state.delta` | `chaoId, version, patch` |
| `evolution` | `chaoId, fromStage, toStage, form` |
| `command.rejected` | `idempotencyKey, code, retryAfterMs` |
| `competition.result` | `runId, placement, rewards[]` |
| `visit.reaction` | `gardenOwnerId, visitorId, kind` |
| `presence` | `userId, online` (Phase 2 only) |

## Sequence — feeding while offline

```
Client (offline): shows predicted hunger decay locally.
Reconnect:        sync {sinceVersion:41}
API:              tick chao, emit state.delta seq 42 (corrects prediction)
                  returns events 42..N
Client:           reconciles, updates widget app-group store
```

## Entity relationships (Postgres, summary)

```
users 1─* chao
users 1─* inventory_items (item_id, qty)
users 1─1 garden (16x16 grid rows)
garden 1─* garden_placements
chao 1─* stat_ledger / care_history
users 1─* currency_ledger
users 1─* command_log (idempotency_key UNIQUE)
competition_runs 1─* competition_entries ─> chao
seasons 1─* competition_runs
```

Full DDL lives with the API migrations; ERD above is the normative reference
for MVP tables.
