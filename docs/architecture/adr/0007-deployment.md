# ADR-0007 — Deployment

**Status:** Accepted

## Local
- `docker compose` provides PostgreSQL 5432 and Redis 6379 (already present).
- API and mobile run from the pnpm workspace; the tick worker runs as a
  second NestJS process with the `TICK_WORKER=1` flag.

## MVP production
- **Compute:** containerized API + WS gateway + tick worker on a managed
  container platform (e.g. Fly.io / ECS), min 2 API instances for zero-downtime
  deploys; gateway and API can be separate services at Phase 2.
- **PostgreSQL:** managed (e.g. Neon/RDS) with PITR, nightly logical backups
  (35-day roll-off), migrations via `drizzle-kit`/`prisma migrate` in the
  deploy pipeline (expand → migrate → contract).
- **Redis:** managed instance; Redis down is a degradation path, not an
  outage (see `../../game-design/anti-cheat.md` failure modes).
- **Object storage:** S3-compatible bucket behind a CDN; assets are immutable
  and content-hashed; uploads use short-lived presigned URLs.
- **Secrets:** platform secret store; `.env.example` documents every variable;
  JWT signing keys in a KMS-backed store with versioned `kid`.
- **Environments:** `dev`, `staging` (prod-like, synthetic data), `prod`.
  Mobile points at an API base URL baked at build time.
- **Deploys:** CI builds images, runs migrations, rolls containers with health
  checks (`/healthz`, readiness includes DB + optional Redis check).

## Scale path (Phases 2–3)
1. Read replicas + cache-heavy visit path; WS gateways split from API pods
   behind a lobby service; per-user routing via pub/sub (no sticky sessions).
2. Tick worker sharded by `userId % N`; Postgres partition `stat_ledger`,
   `user_events` by month; regional read caches for snapshots.
3. Global competition ledger; multi-region read scaling, single write region
   (write locality via account home region).

## Consequences
- Simple ops at MVP; regional scaling deferred until metrics demand it.
- Outbox-backed events (ADR-0003) make deploy-time failover safe.
