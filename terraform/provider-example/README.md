# POP3 Server Terraform Example

This directory contains an example Terraform configuration for managing POP3 server domains and users via the REST API.

## Prerequisites

1. POP3 server running with API enabled
2. API key configured in `config.toml`
3. `curl` command available

## Usage

1. Initialize Terraform:

```bash
terraform init
```

2. Set your API key:

```bash
export TF_VAR_pop3_api_key="your-api-key"
```

3. Review the plan:

```bash
terraform plan
```

4. Apply the configuration:

```bash
terraform apply
```

## Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `pop3_api_endpoint` | API endpoint URL | `http://127.0.0.1:8080` |
| `pop3_api_key` | API authentication key | (required) |
| `domain_name` | Domain to create | `example.com` |
| `users` | Map of users to create | See main.tf |

## Custom Provider

For production use, consider creating a custom Terraform provider. The REST API supports:

- `GET/POST /api/v1/domains` - List/Create domains
- `GET/DELETE /api/v1/domains/{domain}` - Get/Delete domain
- `GET/POST /api/v1/domains/{domain}/users` - List/Create users
- `GET/PUT/DELETE /api/v1/domains/{domain}/users/{user}` - Get/Update/Delete user

Example provider resource definitions:

```hcl
resource "pop3server_domain" "example" {
  name            = "example.com"
  default_storage = "maildir"
  maildir_base    = "/var/mail/example.com"
  cert_path       = "certs/example.com.crt"
  key_path        = "certs/example.com.key"
}

resource "pop3server_user" "alice" {
  domain      = pop3server_domain.example.name
  username    = "alice"
  password    = var.alice_password
  webhook_url = "https://hook.example.com/alice"
}
```
