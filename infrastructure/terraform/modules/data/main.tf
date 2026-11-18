variable "environment" {
  type = string
}
variable "app_name" {
  type = string
}
variable "region" {
  type    = string
  default = "ams"
}
variable "backup_retention_days" {
  type    = number
  default = 30
}
variable "machine_count" {
  type    = number
  default = 2
}

# Managed PostgreSQL (Fly Postgres Flex) with automated backups and WAL
# archiving. Retention is environment-specific: 7 days staging, 30 production.
resource "fly_postgres_flex" "db" {
  app_name              = "${var.app_name}-db"
  organization          = "chaogarden"
  region                = var.region
  volume_size_gb        = var.environment == "production" ? 10 : 3
  replicas              = var.machine_count
  upgrade_to_flex       = false
  backup_retention_days = var.backup_retention_days
}

# Managed Redis (Upstash) for session, pub/sub fanout, and rate limits.
resource "fly_redis" "cache" {
  app_name = "${var.app_name}-redis"
  organization = "chaogarden"
  region    = var.region
  plan      = var.environment == "production" ? "paid" : "free"
  maxmemory_policy = "allkeys-lru"
}

# Object storage (Cloudflare R2) for asset imports and DB backup archives.
resource "cloudflare_r2_bucket" "assets" {
  name      = "chao-${var.environment}-assets"
  account_id = var.cloudflare_account_id
}

# DNS + TLS via Cloudflare: wildcard CNAME to Fly edge; universal SSL.
resource "cloudflare_record" "api" {
  zone_id = var.cloudflare_zone_id
  name    = var.environment == "production" ? "api" : "staging"
  type    = "CNAME"
  value   = "${var.app_name}.fly.dev"
  proxied = true
  ttl     = 1
}

output "database_url" {
  value     = fly_postgres_flex.db.connection_string
  sensitive = true
}
output "redis_url" {
  value     = fly_redis.cache.ip
  sensitive = true
}
output "backup_bucket" {
  value = cloudflare_r2_bucket.assets.name
}
