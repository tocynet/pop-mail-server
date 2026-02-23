#!/bin/bash
# Generate self-signed TLS certificates for development/testing
# Usage: ./scripts/generate-certs.sh [domain]

set -e

DOMAIN="${1:-localhost}"
CERTS_DIR="certs"
DAYS=365

echo "Generating self-signed certificates for: $DOMAIN"

# Create certs directory if it doesn't exist
mkdir -p "$CERTS_DIR"

# Generate private key
openssl genrsa -out "$CERTS_DIR/server.key" 2048

# Generate certificate signing request (CSR)
openssl req -new \
    -key "$CERTS_DIR/server.key" \
    -out "$CERTS_DIR/server.csr" \
    -subj "/C=JP/ST=Tokyo/L=Tokyo/O=Development/OU=POP3 Server/CN=$DOMAIN"

# Generate self-signed certificate
openssl x509 -req \
    -days $DAYS \
    -in "$CERTS_DIR/server.csr" \
    -signkey "$CERTS_DIR/server.key" \
    -out "$CERTS_DIR/server.crt" \
    -extfile <(printf "subjectAltName=DNS:$DOMAIN,DNS:localhost,IP:127.0.0.1")

# Remove CSR (not needed)
rm "$CERTS_DIR/server.csr"

# Set appropriate permissions
chmod 600 "$CERTS_DIR/server.key"
chmod 644 "$CERTS_DIR/server.crt"

echo ""
echo "Certificates generated successfully:"
echo "  Certificate: $CERTS_DIR/server.crt"
echo "  Private Key: $CERTS_DIR/server.key"
echo ""
echo "Certificate details:"
openssl x509 -in "$CERTS_DIR/server.crt" -noout -subject -dates
echo ""
echo "WARNING: These are self-signed certificates for development only."
echo "         Do NOT use in production. Use Let's Encrypt or similar for production."
