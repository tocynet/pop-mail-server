# POP3 Mail Server

Rustで実装されたセキュアなPOP3メール受信サーバー。

## 機能

- **POP3プロトコル**: RFC 1939準拠 (USER, PASS, STAT, LIST, RETR, DELE, QUIT, NOOP, RSET, UIDL, TOP, CAPA)
- **TLS必須**: ポート995でのセキュア接続のみ対応
- **ストレージ**: Maildir形式 + S3（オプション）
- **バーチャルホスト**: 複数ドメイン対応 (`user@domain`形式)
- **Webhook**: 全コマンドの通知機能
- **IMAP転送**: 受信メールのIMAP自動転送
- **管理API**: REST APIによるドメイン/ユーザー管理
- **CLI**: `pop3ctl`コマンドラインツール

## クイックスタート

### 1. ビルド

```bash
cargo build --release
```

### 2. TLS証明書の生成（開発用）

```bash
./scripts/generate-certs.sh localhost
```

### 3. ユーザーの作成

```bash
# パスワードハッシュを生成
./target/release/pop3ctl hash-password
# Password: (入力)
# $argon2id$v=19$...

# users.tomlを編集してハッシュを設定
```

### 4. サーバー起動

```bash
./target/release/pop3-server --config config.toml --users users.toml
```

## Docker

```bash
# ビルド
docker build -t pop3-server .

# 証明書を生成
./scripts/generate-certs.sh

# 起動
docker-compose up -d
```

## 設定

### config.toml

```toml
[server]
bind_address = "0.0.0.0:995"
max_connections = 100

[tls]
cert_path = "certs/server.crt"
key_path = "certs/server.key"

[security]
command_timeout_secs = 30
idle_timeout_secs = 300
rate_limit_per_second = 10
max_auth_attempts = 5

[storage]
default_type = "maildir"
maildir_base = "/var/mail"

[api]
enabled = true
bind_address = "127.0.0.1:8080"
api_key = "your-secret-api-key"
```

### users.toml

```toml
[[users]]
username = "alice"
domain = "example.com"
password_hash = "$argon2id$v=19$..."
maildir = "/var/mail/example.com/alice"
storage = "maildir"
webhook_url = "https://example.com/webhook/alice"
enabled = true
```

## CLI (pop3ctl)

```bash
# ドメイン管理
pop3ctl domain list
pop3ctl domain add example.com
pop3ctl domain remove example.com

# ユーザー管理
pop3ctl user list example.com
pop3ctl user add alice@example.com --password-stdin
pop3ctl user remove alice@example.com
pop3ctl user set-webhook alice@example.com https://hook.example.com

# パスワードハッシュ生成
pop3ctl hash-password

# ヘルスチェック
pop3ctl health
```

## REST API

| エンドポイント | メソッド | 説明 |
|---------------|---------|------|
| `/api/v1/health` | GET | ヘルスチェック |
| `/api/v1/domains` | GET/POST | ドメイン一覧/作成 |
| `/api/v1/domains/{domain}` | GET/DELETE | ドメイン詳細/削除 |
| `/api/v1/domains/{domain}/users` | GET/POST | ユーザー一覧/作成 |
| `/api/v1/domains/{domain}/users/{user}` | GET/PUT/DELETE | ユーザー管理 |

認証: `X-API-Key` ヘッダー

```bash
curl -H "X-API-Key: your-secret-api-key" http://localhost:8080/api/v1/domains
```

## テスト

```bash
# ユニットテスト
cargo test

# 手動テスト（OpenSSL）
openssl s_client -connect localhost:995
```

## POP3コマンド例

```
+OK POP3 server ready
USER alice@example.com
+OK User accepted
PASS secret
+OK Authentication successful
STAT
+OK 3 12345
LIST
+OK Message list follows
1 1234
2 5678
3 5433
.
RETR 1
+OK 1234 octets
(メール本文)
.
QUIT
+OK Bye
```

## デプロイ

### Oracle Cloud Free Tier（推奨）

1. ARM VMインスタンスを作成
2. ポート995, 8080を開放
3. Dockerをインストール
4. 本番用TLS証明書を設定（Let's Encrypt推奨）
5. `docker-compose up -d`

### 本番用TLS証明書

```bash
# Let's Encryptの場合
certbot certonly --standalone -d mail.example.com

# 証明書パスを設定
# cert_path = "/etc/letsencrypt/live/mail.example.com/fullchain.pem"
# key_path = "/etc/letsencrypt/live/mail.example.com/privkey.pem"
```

## ライセンス

MIT
