# Chao Garden — Terraform (Fly.io provider-native IaC)

Provider-native equivalent chosen in docs/architecture ADR: Fly.io for the
container platform + managed Postgres/Flex + Upstash Redis, Cloudflare for DNS,
TLS, and object storage (R2). Selected over a full Terraform-module tree to keep
MVP cost near zero: Fly machines scale to zero, Postgres and Redis are billed
per-use, and R2 has no egress fees.

```
infrastructure/terraform/
  modules/
    api/        # app, machines, Postgres attach, Redis, secrets, checks
    data/       # Postgres + Redis + backups + storage bucket + DNS/TLS
  envs/
    staging/
    production/
```

## Layout

- `modules/api` — Fly app, machine sizes (shared-cpu-1x / 256MB staging,
  shared-cpu-1x / 512MB production), `fly_postgres` attachment, Upstash Redis
  via `fly_redis`, secrets referenced from GitHub environment secrets (never
  committed), HTTP checks on `/livez` and `/readyz`, `min_machines_running = 1`
  (staging 0 to save cost), release command for migrations, and
  `release_command` rollback via redeploying the previous digest.
- `modules/data` — Postgres cluster with `PG_BACKUP_RETENTION_DAYS = 30`
  (staging 7), R2 bucket + lifecycle rule, Cloudflare DNS record and
  universal-SSL TLS, Upstash Redis (Fly integration).

## Environments

`envs/staging` and `envs/production` share identical modules; only variables
differ (machine size, replica count, backup retention, alert thresholds).
State lives in a remote GCS backend per environment — run `terraform init` with
`-backend-config` pointing at the right bucket before plan/apply.

## Apply

```sh
cd infrastructure/terraform/envs/staging
terraform init -backend-config=backend.hcl
terraform plan
terraform apply
```

CI applies via OIDC (`.github/workflows/deploy.yml`) with staging and
production GitHub environment approval gates.

## Secrets reference (never stored in repo)

`DATABASE_URL`, `REDIS_URL`, `SENTRY_DSN`, `SESSION_SIGNING_KEY`,
`OBJECT_STORAGE_ACCESS_KEY`, `OBJECT_STORAGE_SECRET_KEY` are set as Fly
secrets by CI using `fly secrets set` from GitHub environment secrets.
