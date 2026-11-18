# Anti-Cheat & Server Authority

## Authority boundary

- **Server owns:** chao stats/needs/stage, inventory, currencies, garden grid,
  competition results, friendship/reaction counts, timestamps.
- **Client owns:** nothing authoritative. The client runs
  `@chaogarden/simulation` for *prediction and rendering only*; any divergence
  is corrected by the next server snapshot.

## Rules

1. **Intents, not outcomes.** Clients send `{type, chaoId, args, idempotencyKey}`.
   The server computes deltas via the shared simulation package. A client that
   sends "chao gained +500 stat" is rejected.
2. **Server time only.** All cooldowns, decay, and schedules use the API server
   clock; decay is computed from `lastTickedAt`, never from client-reported
   elapsed time.
3. **Idempotency.** Every mutating command carries a client-generated
   idempotency key stored in Postgres (`command_log`); replays return the
   original result.
4. **Rate limits (Redis token buckets):**
   - 60 gameplay commands/min per user.
   - Per-action cooldowns enforced server-side (see `mvp-rules.md`).
   - Visits: 100 garden fetches/h per user.
5. **Ledger audit.** Currencies and stat gains append to
   `currency_ledger` / `stat_ledger` with actor, reason code, and balance
   after. Anomaly jobs flag: gains > statistical envelope, impossible deltas,
   duplicate idempotency keys, rapid device/IP churn on one account.
6. **Validation layers.** Zod schema (contracts) → semantic rules (simulation
   preconditions) → rate limit → persistence, in that order; each failure
   returns a typed error event.
7. **No trust in upload metadata.** Player-uploaded decoration images are
   limited to server-generated content or curated catalog items in MVP;
   free uploads are Phase 2 with content scanning and re-encoding.
8. **Competition integrity.** Runs execute in a single worker from a frozen
   snapshot of the chao taken at enrollment close; late stat changes cannot
   affect an enrolled run.

## Failure modes

| Failure | Behavior |
| --- | --- |
| WS disconnect | Client falls back to REST + local prediction; state reconciles on reconnect |
| Redis down | API degrades: cooldowns enforced via Postgres fallback, pub/sub fanout paused, visits served stale snapshots |
| Tick worker crash | Ticks resume idempotently from `lastTickedAt`; at-least-once with dedupe by chao tick version |
| Duplicate command replay | Idempotency key returns cached result |
| Clock skew across API nodes | All nodes use NTP; drift > 500 ms alerts and the node drains |
