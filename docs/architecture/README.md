# ChaoGarden MVP — Architecture

ChaoGarden is an original creature-raising MMO with a phone-widget "tiny garden".
**Every production name, visual, audio asset, and piece of lore must be original.
No Sega or Sonic Adventure 2 assets (models, textures, music, sounds, characters,
trademarks) may be bundled, shipped, or referenced by production clients.** The
existing prototype tooling that imports owner-supplied game files lives in
separate repositories and is never part of a production distribution.

## Documents

| Document | Purpose |
| --- | --- |
| [system-overview.md](system-overview.md) | Components, state ownership, data flow diagrams, offline behavior |
| [adr/](adr/) | Architecture decision records |
| [../game-design/mvp-rules.md](../game-design/mvp-rules.md) | Gameplay loops, formulas, progression, currencies |
| [../game-design/anti-cheat.md](../game-design/anti-cheat.md) | Server authority boundaries and anti-cheat rules |
| [../game-design/privacy.md](../game-design/privacy.md) | Privacy & data-retention requirements |

## Stack (MVP)

- **Client:** React Native (Expo) mobile app + native Android/iOS widgets.
- **Backend:** TypeScript (NestJS) API + WebSocket gateway.
- **Data:** PostgreSQL (authoritative state), Redis (presence, rate limits,
  pub/sub fan-out, session cache), S3-compatible object storage (original
  artwork, audio, garden decorations, player uploads).
- **Simulation:** `@chaogarden/simulation` — pure deterministic TypeScript
  package shared by server and client (client uses it for prediction only).

## MVP scope

Loops shipped in MVP: adopt, feed, train, rest, evolve, decorate, visit other
gardens (async snapshot view), asynchronous competitions (judged server-side on
a schedule, no live PvP). Explicitly out of scope: real-time multiplayer in one
garden, trading, chat.

## Scale path

Phase 1 (MVP): single API deployment, one Postgres, one Redis, async
multiplayer only. Phase 2: read replicas + regional read caching, sharding of
gardens by player ID, dedicated WS gateways behind a lobby service. Phase 3:
sim tick workers as an autoscaled queue consumer, global competition ledger,
CDN-fronted object storage, cross-region Postgres with partitioned gameplay
tables. See `adr/0007-deployment.md` and `adr/0008-observability.md`.
