module "api" {
  source = "../../modules/api"

  app_name           = "chao-api-staging"
  image              = var.image
  machine_size       = "shared-cpu-1x"
  memory_mb          = 256
  min_machines       = 0
  region             = "ams"
  database_url       = var.database_url
  redis_url          = var.redis_url
  sentry_dsn         = var.sentry_dsn
  allowed_origins    = ["https://staging.chaogarden.example"]
  environment        = "staging"
  release_command    = "/usr/local/bin/chao-server migrate"
  internal_port      = 3001
}
