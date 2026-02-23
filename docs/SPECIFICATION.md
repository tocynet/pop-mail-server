# POP3 Mail Server 仕様書

## 概要

Rustで実装されたセキュアなPOP3メール受信サーバー。個人やスタートアップ向けに、シンプルで安全なメール受信環境を提供する。

## 想定ユースケース

### 1. 個人メールサーバー
- 独自ドメインでのメール受信
- Oracle Cloud Free Tier等の無料VPSでの運用
- Gmail等への自動転送

### 2. スタートアップ/小規模チーム
- 複数ドメインの一元管理（バーチャルホスト）
- チームメンバーごとのメールボックス
- Webhook連携による業務自動化

### 3. IoT/システム連携
- 機器からのメール通知受信
- Webhook経由での外部システム連携
- S3ストレージによるスケーラブルな保存

### 4. 開発/テスト環境
- ローカルでのメール受信テスト
- CI/CDパイプラインでのメール機能テスト

---

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────┐
│                   POP3 Mail Server                       │
├─────────────────────────────────────────────────────────┤
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐   │
│  │ Server  │  │ VHost   │  │  Auth   │  │ Storage │   │
│  │(TLS必須)│→│ Router  │→│(Argon2) │→│ Adapter │   │
│  └─────────┘  └─────────┘  └─────────┘  └────┬────┘   │
│       │                                       │         │
│       │           ┌───────────────────────────┤         │
│       │           │                           │         │
│       ▼           ▼                           ▼         │
│  ┌─────────┐  ┌─────────┐              ┌─────────┐     │
│  │ Webhook │  │  IMAP   │              │ Maildir │     │
│  │Dispatch │  │ Forward │              │   / S3  │     │
│  └─────────┘  └─────────┘              └─────────┘     │
├─────────────────────────────────────────────────────────┤
│  ┌─────────┐  ┌─────────┐                              │
│  │ REST API│  │ pop3ctl │                              │
│  │ (axum)  │  │  (CLI)  │                              │
│  └─────────┘  └─────────┘                              │
└─────────────────────────────────────────────────────────┘
```

---

## コア機能

### 1. POP3プロトコル (RFC 1939)

| コマンド | 説明 |
|---------|------|
| USER | ユーザー名指定（`user@domain`形式） |
| PASS | パスワード認証 |
| STAT | メッセージ数とサイズ取得 |
| LIST | メッセージ一覧 |
| RETR | メッセージ取得 |
| DELE | メッセージ削除マーク |
| QUIT | セッション終了（削除実行） |
| NOOP | 接続維持 |
| RSET | 削除マークのリセット |
| UIDL | メッセージ固有ID一覧 |
| TOP  | ヘッダー＋本文n行取得 |
| CAPA | サーバー機能一覧 |

### 2. セキュリティ

| 機能 | 実装 |
|------|------|
| 暗号化 | TLS必須（ポート995、暗黙的TLS） |
| 認証 | Argon2idハッシュ |
| レート制限 | 秒間コマンド数制限 |
| タイムアウト | コマンド/アイドルタイムアウト |
| 認証試行制限 | 最大試行回数制限 |
| メモリ保護 | パスワードのzeroize |

### 3. バーチャルホスト

複数ドメインを1サーバーで運用可能。

```
alice@example.com  → /var/mail/example.com/alice/
bob@company.jp     → /var/mail/company.jp/bob/
```

ドメイン設定ファイル: `domains/{domain}.toml`

### 4. ストレージアダプタ

| タイプ | 用途 |
|--------|------|
| Maildir | ローカルファイルシステム（標準） |
| S3 | AWS S3 / MinIO（スケーラブル） |

ユーザーごとにストレージタイプを選択可能。

### 5. Webhook通知

メール操作をリアルタイムに外部システムへ通知。

**対応イベント:**
- `session_start` - ログイン
- `session_end` - ログアウト
- `command_retr` - メール取得
- `command_dele` - メール削除

**ペイロード例:**
```json
{
  "event": "command_retr",
  "user": "alice@example.com",
  "timestamp": "2024-01-15T10:30:00Z",
  "message_id": "1",
  "message_size": 12345
}
```

### 6. IMAP転送

受信メールを別のIMAPサーバー（Gmail等）に自動転送。

```toml
[users.imap_forward]
enabled = true
host = "imap.gmail.com"
port = 993
tls = true
username = "alice@gmail.com"
password_env = "ALICE_IMAP_PASSWORD"
target_folder = "INBOX"
delete_after_forward = false
```

### 7. 管理API（自動永続化）

RESTfulなAPI（axum製）でドメイン/ユーザーを管理。
**API経由の変更は自動的に`users.toml`に保存されます。**

| エンドポイント | メソッド | 説明 |
|---------------|---------|------|
| `/api/v1/health` | GET | ヘルスチェック |
| `/api/v1/domains` | GET/POST | ドメイン一覧/作成 |
| `/api/v1/domains/{domain}` | GET/DELETE | ドメイン詳細/削除 |
| `/api/v1/domains/{domain}/users` | GET/POST | ユーザー一覧/作成 |
| `/api/v1/domains/{domain}/users/{user}` | GET/PUT/DELETE | ユーザー管理 |

認証: `X-API-Key` ヘッダー

### 8. ホットリロード

`users.toml`ファイルの変更を自動検知し、サーバー再起動なしで反映。

**仕組み:**
```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  API変更    │────→│ users.toml   │────→│ ホット      │
│  (upsert/   │     │ (自動保存)   │     │ リロード    │
│   remove)   │     └──────────────┘     │ (自動検知)  │
└─────────────┘            ↑             └─────────────┘
                           │                    │
                    ┌──────┴──────┐             │
                    │  手動編集   │             │
                    │  (vim等)    │─────────────┘
                    └─────────────┘
```

**整合性保証:**

| 操作 | 動作 | 遅延 |
|------|------|------|
| API変更 | ファイルに自動保存 | 即座（500ms以内） |
| ファイル変更 | メモリにリロード | 100ms後 |
| 自己トリガー | 無視 | - |

自己トリガー防止により、API保存直後のファイル変更イベントは無視され、二重リロードを防ぎます。

### 8. CLI (pop3ctl)

```bash
pop3ctl domain list              # ドメイン一覧
pop3ctl domain add example.com   # ドメイン追加
pop3ctl user add alice@example.com --password-stdin
pop3ctl user set-webhook alice@example.com https://hook.example.com
pop3ctl hash-password            # パスワードハッシュ生成
pop3ctl health                   # ヘルスチェック
```

---

## 設定ファイル

### config.toml（サーバー設定）

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

[storage.s3]
region = "ap-northeast-1"
endpoint = ""  # MinIO等のカスタムエンドポイント

[webhook]
timeout_secs = 10
retry_count = 3

[api]
enabled = true
bind_address = "127.0.0.1:8080"
api_key = "your-secret-api-key"
```

### users.toml（ユーザー設定）

```toml
[[users]]
username = "alice"
domain = "example.com"
password_hash = "$argon2id$v=19$..."
maildir = "/var/mail/example.com/alice"
storage = "maildir"
webhook_url = "https://example.com/webhook/alice"
webhook_events = ["session_start", "command_retr"]
enabled = true

[users.imap_forward]
enabled = true
host = "imap.gmail.com"
port = 993
tls = true
username = "alice@gmail.com"
password_env = "ALICE_IMAP_PASSWORD"
```

---

## 技術スタック

| カテゴリ | ライブラリ |
|---------|-----------|
| 非同期ランタイム | tokio |
| TLS | tokio-rustls, rustls |
| メールストレージ | maildir |
| 認証 | argon2 |
| 設定 | toml, serde |
| レート制限 | governor |
| ロギング | tracing |
| S3 | aws-sdk-s3 (optional) |
| Webhook | reqwest |
| API | axum, tower |
| CLI | clap |
| IMAP | async-imap |
| ファイル監視 | notify |

---

## デプロイメント

### 推奨環境

| 環境 | スペック | 月額 |
|------|---------|------|
| Oracle Cloud Free Tier | ARM 4コア, 24GB RAM | 無料 |
| AWS Lightsail | 1vCPU, 1GB RAM | $5〜 |
| DigitalOcean | 1vCPU, 1GB RAM | $6〜 |

### 必要ポート

| ポート | プロトコル | 用途 |
|-------|-----------|------|
| 995 | TCP/TLS | POP3S（メール受信） |
| 8080 | TCP | 管理API（内部のみ推奨） |

### Dockerデプロイ

```bash
docker build -t pop3-server .
docker-compose up -d
```

### TLS証明書

**開発用（自己署名）:**
```bash
./scripts/generate-certs.sh localhost
```

**本番用（Let's Encrypt）:**
```bash
certbot certonly --standalone -d mail.example.com
```

---

## 制限事項

- **受信専用**: SMTP（送信）機能は含まない
- **TLS必須**: 平文POP3（ポート110）は非対応
- **シングルノード**: クラスタリング非対応（S3使用で擬似的に可能）

---

## ロードマップ

- [ ] APOP認証対応
- [ ] メトリクス（Prometheus）
- [ ] 管理WebUI
- [ ] Let's Encrypt自動更新
- [ ] クラスタモード
