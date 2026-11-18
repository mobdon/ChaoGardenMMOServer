# ADR-0008 — Observability

**Status:** Accepted

## Decision
- **Metrics:** OpenTelemetry SDK exported to a metrics backend (Prometheus-
  compatible). Key signals:
  - RED per endpoint/WS method: rate, errors, duration.
  - Gameplay: commands/min by type, command rejection reasons, tick lag
    (`now - next_tick_at` of oldest due chao), outbox replay depth, currency
    net-flow per source/sink.
  - Client (aggregated, opt-in): sync latency, prediction drift magnitude,
    widget refresh success, crash-free sessions.
- **Traces:** OTLP traces across API → Postgres/Redis/worker with sampled
  payloads (no PII, no raw request bodies).
- **Logs:** structured JSON, `userId` only (never email), request ID
  correlation; 30-day retention for app logs, 7-day for debug level.
- **Alerts (MVP):**
  - tick lag > 5 min; outbox backlog > 10k; WS disconnect rate spike (>3× 7-day
    baseline); 5xx rate > 1%; Redis unavailable > 2 min; Postgres replication/
    backup failure; anomaly-job queue depth.
- **Dashboards:** gameplay health, economy flow, infra saturation, mobile
  stability.
- **Anomaly jobs:** scheduled queries over ledgers flag statistical outliers
  (see `../../game-design/anti-cheat.md`) into a review queue.

## Consequences
- Business-relevant signals (economy, ticks) are first-class, not an
  afterthought.
- No raw gameplay payloads in observability systems, keeping PII posture
  clean (`../../game-design/privacy.md`).
