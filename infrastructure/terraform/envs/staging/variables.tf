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
