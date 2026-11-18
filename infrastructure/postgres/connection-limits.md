# Configure DB connection limits

PostgreSQL default `max_connections = 100` is too generous for the API's
shared-machine sizing and hides saturation until failures surface. Tune per
environment at provision time (managed providers expose these via config, not
`ALTER SYSTEM` on Fly Postgres Flex):

| Setting                | Staging | Production | Rationale                                       |
| ---------------------- | ------- | ---------- | ----------------------------------------------- |
| `max_connections`      | 50      | 100        | API pool + release command + admin headroom     |
| `shared_buffers`       | 128MB   | 256MB      | Matches attached memory                          |
| `idle_in_transaction_session_timeout` | 30s | 30s | Prevents stuck transactions from exhausting pool |
| `statement_timeout`    | 10s     | 10s        | API queries only; migrations override to 0       |

The API opens its pool with `pgx` using:

- `max_size = 10` per machine (2 machines → 20 of 100 in production)
- `acquire_timeout = 2s` so saturation surfaces as a fast 503, not a hang
- pool is closed on SIGTERM before graceful shutdown completes

Monitoring: `pg_stat_activity` count vs `max_connections` is exported as
`db_connections_used_ratio`; alert at 0.8 (see `infrastructure/monitoring/`).
