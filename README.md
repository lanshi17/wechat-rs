<p align="center">
  <h1 align="center">wechat-rs</h1>
  <p align="center">
    <strong>A high-performance WeChat Official Account backend service built with Rust</strong>
  </p>
  <p align="center">
    <a href="https://github.com/lanshi17/wechat-rs/actions/workflows/rust.yml">
      <img src="https://github.com/lanshi17/wechat-rs/actions/workflows/rust.yml/badge.svg" alt="Build">
    </a>
    <a href="https://hub.docker.com/r/davepaine/wechat-rs">
      <img src="https://img.shields.io/docker/pulls/davepaine/wechat-rs" alt="Docker Pulls">
    </a>
    <a href="https://github.com/lanshi17/wechat-rs/pkgs/container/wechat-rs">
      <img src="https://ghcr-badge.egpl.dev/lanshi17/wechat-rs/latest_tag?trim=major&label=GHCR" alt="GHCR">
    </a>
    <a href="https://github.com/lanshi17/wechat-rs/blob/main/LICENSE">
      <img src="https://img.shields.io/github/license/lanshi17/wechat-rs" alt="License">
    </a>
    <a href="https://github.com/lanshi17/wechat-rs">
      <img src="https://img.shields.io/github/stars/lanshi17/wechat-rs?style=social" alt="Stars">
    </a>
  </p>
</p>

---

## Why wechat-rs?

Building a WeChat Official Account backend usually means stitching together Python or Node.js scripts, wrestling with XML parsing, and managing fragile integrations. **wechat-rs gives you a production-ready backend in a single binary.**

- 🚀 **Deploy in 30 seconds** — one `docker-compose up` and you're live
- ⚡ **Rust-powered performance** — Axum + Tokio async runtime, minimal memory footprint
- 🛡️ **Production security built-in** — strict callback verification, JWT auth, bcrypt, and AES-encrypted messages
- 🎛️ **Full admin dashboard** — real-time stats, user management, config sync, no extra tooling needed
- 🔌 **Pluggable storage** — PostgreSQL or Redis, switch with a single config line

## Quick Start

```bash
# Clone and configure
git clone https://github.com/lanshi17/wechat-rs.git
cd wechat-rs
cp config.toml.example config.toml
# Edit config.toml with your database URL, admin password, WeChat credentials,
# and a long random upstream.server_token

# Run with Docker (pulls image automatically)
docker-compose up -d

# Open admin dashboard
# → http://localhost:3317/admin/
```

That's it. PostgreSQL tables are created automatically on first startup.

> **Required:** Replace the example admin password and JWT secret before first startup. The service refuses the known defaults (and secrets shorter than 32 bytes); the password is bcrypt-hashed on first login.

## Features

| Feature | Details |
|---------|---------|
| **WeChat Messaging** | Subscribe/unsubscribe, text/help, voice recognition, image/location replies, menu clicks |
| **Verification Codes** | Collision-safe 6-digit codes, 3-minute TTL, atomic one-time consumption |
| **Admin Dashboard** | Real-time stats, user search, code audit logs, health monitoring |
| **Config Sync** | Edit WeChat credentials via UI — auto-synced to `config.toml` |
| **Storage** | PostgreSQL and Redis backends via trait-based abstraction |
| **Security** | Strict plaintext/AES callback signatures, AppID validation, JWT, fail-closed service tokens |
| **Deployment** | Single binary or Docker image, Nginx reverse proxy ready |

## Architecture

```
src/
├── main.rs              # Entry point, config loading, routing
├── api.rs               # Upstream API (one-time code consumption)
├── crypto.rs            # AES encryption and signature verification
├── wechat.rs            # Webhook handlers and message processing
├── admin/
│   ├── mod.rs           # JWT authentication and routing
│   ├── handlers.rs      # Admin API handlers
│   └── ui.rs            # Embedded admin web UI
├── monitor/             # Metrics, events, and alert rules
├── notify/              # Email, SMS, and webhook notifications
└── storage/
    ├── mod.rs           # Storage trait definition
    ├── postgres.rs      # PostgreSQL implementation
    └── redis_store.rs   # Redis implementation
```

**Storage backends** are interchangeable via the `Storage` trait. Both PostgreSQL and Redis enforce atomic, one-time code consumption; Redis uses a TTL lookup plus persistent audit records.

## Configuration

Copy `config.toml.example` and fill in your values:

```toml
[server]
listen_addr = "0.0.0.0:3000"
site_name   = "微信服务管理后台"
domain      = "localhost"

[admin]
password = "admin123"                              # Bcrypt hashed on first login
secret   = "please_change_this_to_a_long_random_string"  # JWT signing secret

[wechat]
token            = ""
appid            = ""
appsecret        = ""
encoding_aes_key = ""

[upstream]
server_token = "replace_with_a_long_random_value"  # Required by upstream endpoints

[storage]
type         = "postgres"    # or "redis"
database_url = "postgres://user:password@host:5432/dbname"
# redis_url  = "redis://localhost:6379"
```

Config path defaults to `./config.toml`. Override with `CONFIG_PATH` env var.

### Admin UI Sync

WeChat credentials, site name, and domain can be edited via the admin UI (`/admin`). Changes sync to both the database and `config.toml`.

## API Reference

### WeChat and Upstream Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/wx` | WeChat server verification (signature, echostr) |
| `POST` | `/wx` | Verified WeChat webhook — plaintext, compatibility, or AES mode |
| `GET` | `/users` | Token-protected user list (`?page=1&size=20`, max size 100) |
| `GET` | `/api/wechat/user?code=XXXXXX` | Atomically consume a code once → returns OpenID |

**Supported events:** `subscribe`, `unsubscribe`, `CLICK`
**Supported messages:** `text`, recognized `voice`, `image`, and `location`. Commands include `验证码`, `verify`, `code`, `帮助`, and `help`.

`/users` and `/api/wechat/user` require `Authorization: Bearer <upstream.server_token>` (the legacy raw token header is also accepted). If the token is empty, both endpoints fail closed with `503 Service Unavailable`. Verification codes become invalid immediately after a successful response.

**Code validation response:**
```json
{
  "success": true,
  "message": "",
  "data": "oXXXXXXXXXXXXXXXXXXXXXXXXX"
}
```

### Admin API

All admin endpoints require JWT (`Authorization: Bearer <token>`).

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/admin/login` | Authenticate, receive JWT token |
| `GET` | `/admin/stats` | Basic subscriber statistics |
| `GET` | `/admin/stats/detailed` | Detailed metrics (daily users, codes, etc.) |
| `GET` | `/admin/health` | System health (memory, DB connections, uptime) |
| `GET` | `/admin/users` | Paginated user list (`?page=1&size=20`) |
| `GET` | `/admin/users/search` | Search users by OpenID (`?q=xxx`) |
| `GET` | `/admin/users/:openid/codes` | Verification history for a user |
| `GET` | `/admin/codes` | Paginated verification code list |
| `GET` | `/admin/config` | Current config (sensitive fields masked) |
| `PUT` | `/admin/config` | Update config fields |
| `POST` | `/admin/menu/create` | Create WeChat custom menu |
| `GET` | `/admin/monitor` | Request metrics, system snapshot, and recent events |
| `POST` | `/admin/monitor/test-alert` | Trigger a notification-channel test |

External monitors can read the non-sensitive liveness snapshot from `GET /api/monitor/status`.

### Admin Web UI

Access at `http://your-domain:3317/admin/`

- **Dashboard** — real-time stats and daily metrics
- **WeChat Config** — Token, AppID, AppSecret management with live validation
- **User Management** — subscriber list with search and verification history
- **Verification Logs** — audit trail with status tracking
- **Security Settings** — password management, end-to-end verification testing
- **System Health** — memory, DB connections, uptime

## Installation

### Prerequisites

- Rust 1.75+ (build from source)
- PostgreSQL 12+ or Redis 6+
- Docker & Docker Compose (recommended)

### Build from Source

```bash
cargo build --release
# Binary at target/release/wechat-rs
```

### Docker

```bash
cp config.toml.example config.toml
# Edit config.toml

docker-compose up -d      # Pulls image from Docker Hub
docker-compose logs -f    # View logs
docker-compose down       # Stop
```

Images are published to two registries — use whichever is faster for you:

- **Docker Hub:** `davepaine/wechat-rs` — https://hub.docker.com/r/davepaine/wechat-rs
- **GHCR (mirror):** `ghcr.io/lanshi17/wechat-rs`

To use GHCR instead, change the image in `docker-compose.yml`:

```yaml
image: ghcr.io/lanshi17/wechat-rs:latest
```

### Build Locally

```bash
cargo build --release
docker build -t wechat-rs:latest .
docker-compose down && docker-compose up -d
```

Docker images for `master` and releases are built and pushed automatically by CI.

## Deployment

### Nginx Reverse Proxy

```nginx
server {
    listen 80;
    server_name your-domain.com;

    location / {
        proxy_pass http://127.0.0.1:3317;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### HTTPS

```bash
certbot --nginx -d your-domain.com
```

### Firewall

```bash
# With Nginx reverse proxy
ufw allow 80/tcp && ufw allow 443/tcp

# Direct access (no proxy)
ufw allow 3317/tcp
```

## Development

```bash
# Run locally
cp config.toml.example config.toml
RUST_LOG=debug cargo run

# Test, format, lint
cargo test
cargo fmt
cargo clippy
```

Logging verbosity via `RUST_LOG` (e.g., `RUST_LOG=wechat_rs=info,tower_http=debug`).

PostgreSQL tables (`wechat_users`, `verification_codes`, `app_config`) are auto-created on startup. Redis requires no setup.

The CI suite runs unit tests plus real PostgreSQL and Redis concurrency tests. To run the ignored backend contracts locally, provide a disposable `WECHAT_RS_TEST_DATABASE_URL`, install `redis-server`, and use `cargo test -- --include-ignored --test-threads=1`.

> **Redis upgrade note:** codes created by versions before this atomic-consumption change have no direct lookup key. During deployment, stop old instances and wait one code TTL (3 minutes), or accept that those in-flight codes become invalid; do not mix old and new instances.

## Roadmap

- [ ] WeChat Mini Program support
- [ ] Template message sending
- [x] Richer message handling (location, image, voice)
- [ ] CLI scaffolding tool for quick project setup
- [ ] Multi-account support

## Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request.

## License

[MIT](LICENSE)
