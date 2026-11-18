variable "app_name" {
  type = string
}
variable "environment" {
  type = string
}
variable "region" {
  type    = string
  default = "ams"
}
variable "machine_size" {
  type    = string
  default = "shared-cpu-1x"
}
variable "memory_mb" {
  type    = number
  default = 512
}
variable "min_machines" {
  type    = number
  default = 0
}
variable "image" {
  type = string
}
variable "database_url" {
  type      = string
  sensitive = true
}
variable "redis_url" {
  type      = string
  sensitive = true
}
variable "sentry_dsn" {
  type      = string
  sensitive = true
}
variable "allowed_origins" {
  type = list(string)
}
variable "release_command" {
  type = string
}
variable "internal_port" {
  type    = number
  default = 3001
}

resource "fly_app" "api" {
  name     = var.app_name
  org      = "chaogarden"
  machinery = "v2"
}

# Health-checked machines: readiness on /readyz, liveness on /livez.
# Machines beyond min_machines are started by the release script
# (scripts/release.sh), which also performs the rolling update.
resource "fly_machine" "api" {
  for_each = toset(var.min_machines > 0 ? ["0", "1"] : ["0"])

  app    = fly_app.api.id
  region = var.region
  name   = "${var.app_name}-${each.key}"

  image = var.image
  services = [
    {
      protocol      = "tcp"
      internal_port = var.internal_port
      ports         = [{ port = 443, handlers = ["http", "tls"] }]
      checks = [
        {
          type         = "http"
          interval     = "15s"
          timeout      = "2s"
          grace_period = "10s"
          method       = "get"
          path         = "/readyz"
        },
        {
          type         = "http"
          interval     = "30s"
          timeout      = "2s"
          grace_period = "10s"
          method       = "get"
          path         = "/livez"
        },
      ]
    },
  ]

  cpus      = 1
  cpu_kind  = var.machine_size
  memory_mb = var.memory_mb

  env = {
    CHAO_ALLOWED_ORIGINS = join(",", var.allowed_origins)
    CHAO_ENV             = var.environment
    RUST_LOG             = "info,chao=debug"
    CHAO_METRICS_ENABLED = "1"
  }

  # Secrets (DATABASE_URL, REDIS_URL, SENTRY_DSN, SESSION_SIGNING_KEY) are set
  # by the deploy pipeline from GitHub environment secrets, never in-repo.
}

# Migrations run as a controlled release step via scripts/migrate.sh before
# machines roll, so a failed migration never reaches the new image.
resource "fly_release_command" "migrate" {
  app     = fly_app.api.id
  command = var.release_command
  image   = var.image
}

output "app_name" {
  value = fly_app.api.name
}
output "release_command" {
  value = fly_release_command.migrate.command
}
