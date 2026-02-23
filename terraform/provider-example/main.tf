# Example Terraform configuration for POP3 Server
#
# This example shows how to use the REST API with Terraform's http provider
# to manage domains and users.
#
# For production use, consider creating a custom Terraform provider.

terraform {
  required_providers {
    http = {
      source  = "hashicorp/http"
      version = "~> 3.0"
    }
    local = {
      source  = "hashicorp/local"
      version = "~> 2.0"
    }
  }
}

variable "pop3_api_endpoint" {
  description = "POP3 server API endpoint"
  type        = string
  default     = "http://127.0.0.1:8080"
}

variable "pop3_api_key" {
  description = "API key for authentication"
  type        = string
  sensitive   = true
}

variable "domain_name" {
  description = "Domain name to create"
  type        = string
  default     = "example.com"
}

variable "users" {
  description = "Map of users to create"
  type = map(object({
    password    = string
    webhook_url = string
  }))
  default = {
    alice = {
      password    = "alice_password_change_me"
      webhook_url = "https://hooks.example.com/alice"
    }
    bob = {
      password    = "bob_password_change_me"
      webhook_url = ""
    }
  }
  sensitive = true
}

# Create domain using REST API
# Note: This is a workaround using null_resource.
# A proper Terraform provider would be better for production use.

resource "null_resource" "domain" {
  provisioner "local-exec" {
    command = <<-EOT
      curl -X POST \
        -H "Content-Type: application/json" \
        -H "X-API-Key: ${var.pop3_api_key}" \
        -d '{"name": "${var.domain_name}", "enabled": true, "default_storage": "maildir", "maildir_base": "/var/mail/${var.domain_name}"}' \
        "${var.pop3_api_endpoint}/api/v1/domains"
    EOT
  }

  provisioner "local-exec" {
    when    = destroy
    command = <<-EOT
      curl -X DELETE \
        -H "X-API-Key: ${self.triggers.api_key}" \
        "${self.triggers.endpoint}/api/v1/domains/${self.triggers.domain}"
    EOT
  }

  triggers = {
    domain   = var.domain_name
    endpoint = var.pop3_api_endpoint
    api_key  = var.pop3_api_key
  }
}

# Create users
resource "null_resource" "users" {
  for_each = var.users

  provisioner "local-exec" {
    command = <<-EOT
      curl -X POST \
        -H "Content-Type: application/json" \
        -H "X-API-Key: ${var.pop3_api_key}" \
        -d '{"username": "${each.key}", "password": "${each.value.password}", "storage": "maildir", "enabled": true, "webhook_url": "${each.value.webhook_url}"}' \
        "${var.pop3_api_endpoint}/api/v1/domains/${var.domain_name}/users"
    EOT
  }

  provisioner "local-exec" {
    when    = destroy
    command = <<-EOT
      curl -X DELETE \
        -H "X-API-Key: ${self.triggers.api_key}" \
        "${self.triggers.endpoint}/api/v1/domains/${self.triggers.domain}/users/${self.triggers.username}"
    EOT
  }

  triggers = {
    username = each.key
    domain   = var.domain_name
    endpoint = var.pop3_api_endpoint
    api_key  = var.pop3_api_key
  }

  depends_on = [null_resource.domain]
}

output "api_health_url" {
  value = "${var.pop3_api_endpoint}/api/v1/health"
}

output "domain" {
  value = var.domain_name
}

output "users" {
  value = keys(var.users)
}
