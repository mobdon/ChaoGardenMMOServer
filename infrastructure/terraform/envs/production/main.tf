terraform {
  backend "gcs" {
    prefix = "chao/production"
  }
  required_providers {
    fly = {
      source  = "fly-apps/fly"
      version = "~> 0.1"
    }
  }
}

provider "fly" {}

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

module "data" {
  source = "../../modules/data"

  environment          = "production"
  app_name             = "chao-api-production"
  region               = "ams"
  backup_retention_days = 30
  machine_count        = 2
}

module "api" {
  source = "../../modules/api"

  app_name           = "chao-api-production"
  image              = var.image
  machine_size       = "shared-cpu-1x"
  memory_mb          = 512
  min_machines       = 1
  region             = "ams"
  database_url       = var.database_url
  redis_url          = var.redis_url
  sentry_dsn         = var.sentry_dsn
  allowed_origins    = ["https://chaogarden.example", "https://www.chaogarden.example"]
  environment        = "production"
  release_command    = "/usr/local/bin/chao-server migrate"
  internal_port      = 3001
  depends_on         = [module.data]
}
