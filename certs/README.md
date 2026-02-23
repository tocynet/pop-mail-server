# TLS Certificates

This directory contains TLS certificates for the POP3 server.

## Development

Generate self-signed certificates for development:

```bash
../scripts/generate-certs.sh localhost
```

This will create:
- `server.crt` - Certificate
- `server.key` - Private key

## Production

For production, use properly signed certificates:

### Let's Encrypt (Recommended)

```bash
certbot certonly --standalone -d mail.example.com
```

Then update `config.toml`:

```toml
[tls]
cert_path = "/etc/letsencrypt/live/mail.example.com/fullchain.pem"
key_path = "/etc/letsencrypt/live/mail.example.com/privkey.pem"
```

### Commercial CA

Place your certificate and key here:
- `server.crt` - Certificate (including intermediate certs)
- `server.key` - Private key

## Security Notes

- Private keys (`*.key`) should have permissions 600
- Never commit real certificates to version control
- The `.gitignore` excludes `*.crt`, `*.key`, and `*.pem` files
