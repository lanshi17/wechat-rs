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
- 🛡️ **Production security built-in** — JWT auth, bcrypt password hashing, AES-encrypted WeChat messages
- 🎛️ **Full admin dashboard** — real-time stats, user management, config sync, no extra tooling needed
- 🔌 **Pluggable storage** — PostgreSQL or Redis, switch with a single config line

## Quick Start

```bash
# Clone and configure
git clone https://github.com/lanshi17/wechat-rs.git
cd wechat-rs
cp config.toml.example config.toml
# Edit config.toml with your database URL, admin password, and WeChat credentials

# Run with Docker (pulls image automatically)
docker-compose up -d

# Open admin dashboard
# → http://localhost:3317/admin/
```

That's it. PostgreSQL tables are created automatically on first startup.

> **Tip:** Set your admin password in `config.toml` under `[admin] password`. It's hashed with bcrypt on first login.

## Features

| Feature | Details |
|---------|---------|
| **WeChat Messaging** | Subscribe/unsubscribe events, text messages, menu clicks |
| **Verification Codes** | 6-digit codes with 3-minute TTL, validation API |
| **Admin Dashboard** | Real-time stats, user search, code audit logs, health monitoring |
| **Config Sync** | Edit WeChat credentials via UI — auto-synced to `config.toml` |
| **Storage** | PostgreSQL and Redis backends via trait-based abstraction |
| **Security** | JWT auth, AES encryption, SHA1 signature verification |
| **Deployment** | Single binary or Docker image, Nginx reverse proxy ready |

## Architecture

```
src/
├── main.rs              # Entry point, config loading, routing
├── api.rs               # Public API (verification code validation)
├── crypto.rs            # AES encryption and signature verification
├── wechat.rs            # Webhook handlers and message processing
├── admin/
│   ├── mod.rs           # JWT authentication and routing
│   ├── handlers.rs      # Admin API handlers
│   └── ui.rs            # Embedded admin web UI
└── storage/
    ├── mod.rs           # Storage trait definition
    ├── postgres.rs      # PostgreSQL implementation
    └── redis_store.rs   # Redis implementation
```

**Storage backends** are interchangeable via the `Storage` trait — switch between PostgreSQL (full SQL, ACID) and Redis (in-memory, auto-TTL) without touching business logic.

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
server_token = ""    # For /api/wechat/user endpoint

[storage]
type         = "postgres"    # or "redis"
database_url = "postgres://user:password@host:5432/dbname"
# redis_url  = "redis://localhost:6379"
```

Config path defaults to `./config.toml`. Override with `CONFIG_PATH` env var.

### Admin UI Sync

WeChat credentials, site name, and domain can be edited via the admin UI (`/admin`). Changes sync to both the database and `config.toml`.

## API Reference

### Public Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/wx` | WeChat server verification (signature, echostr) |
| `POST` | `/wx` | WeChat webhook — messages and events |
| `GET` | `/users` | Paginated user list (`?page=1&size=20`) |
| `GET` | `/api/wechat/user?code=XXXXXX` | Validate verification code → returns OpenID |

**Supported events:** `subscribe`, `unsubscribe`, `CLICK`
**Supported messages:** `text` (commands: "验证码", "verify", "code")

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

## Roadmap

- [ ] WeChat Mini Program support
- [ ] Template message sending
- [ ] Richer event handling (location, image, voice)
- [ ] CLI scaffolding tool for quick project setup
- [ ] Multi-account support

## Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request.

## License

[MIT](LICENSE)
