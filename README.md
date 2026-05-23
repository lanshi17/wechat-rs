# wechat-rs

A high-performance WeChat Official Account backend service written in Rust, providing webhook handling, verification code generation, and a full-featured admin dashboard.

## Quick Start

```bash
# 1. Clone and configure
git clone <repository-url>
cd wechat_sever
cp config.toml.example config.toml
# Edit config.toml with your database URL, admin password, and WeChat credentials

# 2. Run with Docker
docker-compose up -d

# 3. Access admin UI
# Open http://localhost:3317/admin/
```

The default admin password is set in `config.toml` under `[admin] password`. After first login, it's hashed with bcrypt and stored in the database.

## Features

- **WeChat Message Processing**: Handle subscribe/unsubscribe events, text messages, and menu clicks
- **Verification Code System**: Generate and validate 6-digit codes with 3-minute expiration
- **Admin Dashboard**: Web-based management interface with:
  - Real-time statistics and monitoring
  - User management with search functionality
  - Verification code audit logs
  - Site customization (name, domain)
  - System health monitoring
  - Password management
- **Flexible Storage**: Support for PostgreSQL and Redis backends
- **Security**: JWT authentication, AES encryption for WeChat messages
- **Performance**: Built with Tokio async runtime, optimized for high concurrency

## Architecture

```
src/
├── main.rs              # Application entry point, config loading, routing
├── api.rs               # Public API endpoints (verification code validation)
├── crypto.rs            # WeChat AES encryption and signature verification
├── wechat.rs            # WeChat webhook handlers and message processing
├── admin/
│   ├── mod.rs           # Admin module entry, JWT authentication, routing
│   ├── handlers.rs      # Admin API handlers (login, config, stats, etc.)
│   └── ui.rs            # Admin web UI (embedded HTML/CSS/JS)
└── storage/
    ├── mod.rs           # Storage trait definition
    ├── postgres.rs      # PostgreSQL implementation
    └── redis_store.rs   # Redis implementation
```

### Storage Abstraction

The service uses a trait-based storage abstraction (`Storage` trait) that allows switching between PostgreSQL and Redis without changing business logic:

- **PostgreSQL**: Full-featured relational storage with SQL queries
- **Redis**: In-memory storage with sorted sets and hash maps

## Installation

### Prerequisites

- Rust 1.75+ (for building from source)
- PostgreSQL 12+ or Redis 6+
- Docker & Docker Compose (recommended for deployment)

### Build from Source

```bash
cargo build --release
```

The binary will be at `target/release/wechat-rs`.

### Docker Deployment

```bash
# 1. Create config file
cp config.toml.example config.toml
# Edit config.toml with your actual values

# 2. Start (pulls image from DockerHub automatically)
docker-compose up -d

# View logs
docker-compose logs -f

# Stop
docker-compose down
```

### Building and Publishing Docker Images

When updating the service, rebuild and push the Docker image:

```bash
# Build the Rust binary
cargo build --release

# Rebuild and restart the container
docker-compose down
docker-compose up -d --build

# Tag and push to DockerHub
docker tag wechat-sever:latest davepaine/wechat-rs:latest
docker push davepaine/wechat-rs:latest
```

The Docker image is available at: https://hub.docker.com/r/davepaine/wechat-rs

## Configuration

Configuration is managed via a TOML file (`config.toml`). Copy `config.toml.example` as a starting point:

```bash
cp config.toml.example config.toml
# Edit config.toml with your actual values
```

The config file path defaults to `./config.toml`, and can be overridden with the `CONFIG_PATH` environment variable.

### Configuration File Structure

```toml
[server]
listen_addr = "0.0.0.0:3000"
site_name   = "微信服务管理后台"
domain      = "localhost"

[admin]
password = "admin123"                              # Initial login password (bcrypt hashed on first login)
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

### Admin UI Sync

WeChat credentials, site name, and domain can also be edited via the admin web UI (`/admin`). Changes are saved to both the database and the `config.toml` file, keeping them in sync.

### Logging

Set the `RUST_LOG` environment variable to control log verbosity:
```bash
RUST_LOG=wechat_rs=info,tower_http=debug
```

## API Endpoints

### Public Endpoints

#### `GET /wx`
WeChat server verification endpoint.

**Query Parameters:**
- `signature`, `timestamp`, `nonce`, `echostr` (standard WeChat verification)
- `msg_signature`, `encrypt` (for encrypted mode)

**Response:** `echostr` on success

#### `POST /wx`
WeChat webhook for receiving messages and events.

**Request Body:** XML message from WeChat

**Supported Events:**
- `subscribe`: Record user subscription
- `unsubscribe`: Mark user as unsubscribed
- `CLICK`: Handle menu clicks (e.g., `GET_VERIFY_CODE`)

**Supported Messages:**
- `text`: Process text commands (e.g., "验证码", "verify", "code")

**Response:** XML reply (encrypted if AES key configured)

#### `GET /api/wechat/user?code=XXXXXX`
Validate verification code and return associated OpenID.

**Headers:**
- `Authorization: <WECHAT_SERVER_TOKEN>` or `Bearer <WECHAT_SERVER_TOKEN>`

**Query Parameters:**
- `code`: 6-digit verification code

**Response:**
```json
{
  "success": true,
  "message": "",
  "data": "oXXXXXXXXXXXXXXXXXXXXXXXXX"
}
```

### Admin API

All admin endpoints require JWT authentication.

#### `POST /admin/login`
Authenticate and receive JWT token.

**Request Body:**
```json
{
  "password": "your-admin-password"
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs..."
}
```

Use the token in subsequent requests:
```bash
Authorization: Bearer <token>
```

#### `GET /admin/stats`
Basic statistics.

**Response:**
```json
{
  "total_subscribers": 1234
}
```

#### `GET /admin/stats/detailed`
Detailed statistics.

**Response:**
```json
{
  "total_subscribers": 1234,
  "total_users": 1500,
  "today_new_users": 42,
  "today_codes": 156,
  "used_codes": 890,
  "expired_codes": 234,
  "total_codes": 5678
}
```

#### `GET /admin/health`
System health check.

**Response:**
```json
{
  "uptime_seconds": 3600,
  "memory_total_mb": 4096,
  "memory_used_mb": 2048,
  "db_connected": true,
  "db_connections": 5
}
```

#### `GET /admin/users?page=1&size=20`
List subscribed users with pagination.

**Response:**
```json
[
  {
    "openid": "oXXXXXXXXXXXXXXXXXXXXXXXXX",
    "nickname": "User Name",
    "headimgurl": "https://...",
    "subscribe": true,
    "created_at": "2026-05-23T10:00:00Z",
    "updated_at": "2026-05-23T12:00:00Z"
  }
]
```

#### `GET /admin/users/search?q=XXX`
Search users by OpenID (partial match).

#### `GET /admin/users/:openid/codes`
Get verification code history for a specific user.

#### `GET /admin/codes?page=1&size=20`
List all verification codes with pagination.

**Response:**
```json
{
  "codes": [
    {
      "id": 123,
      "openid": "oXXXXXXXXXXXXXXXXXXXXXXXXX",
      "code": "123456",
      "purpose": "",
      "used": false,
      "created_at": "2026-05-23T10:00:00Z",
      "expires_at": "2026-05-23T10:03:00Z"
    }
  ],
  "total": 5678
}
```

#### `GET /admin/config`
Get current configuration (sensitive fields masked).

**Response:**
```json
{
  "wechat_token": "your-token",
  "wechat_appid": "wx1234567890",
  "wechat_appsecret_masked": "abc1****",
  "wechat_encoding_aes_key": "abcd****",
  "welcome_message": "感谢关注！",
  "site_name": "微信服务管理后台",
  "domain": "your-domain.com",
  "has_password": true
}
```

#### `PUT /admin/config`
Update configuration.

**Request Body:**
```json
{
  "wechat_token": "new-token",
  "wechat_appid": "new-appid",
  "wechat_appsecret": "new-secret",
  "welcome_message": "新的欢迎语",
  "site_name": "新站点名称",
  "domain": "new-domain.com",
  "new_password": "new-admin-password"
}
```

All fields are optional. Only provided fields will be updated.

#### `POST /admin/menu/create`
Create WeChat custom menu (requires valid AppID and AppSecret).

**Response:**
```json
{
  "success": true,
  "message": "菜单创建成功"
}
```

### Admin Web UI

Access the admin dashboard at:
```
http://your-domain:3317/admin/
```

Features:
- **Dashboard**: Real-time statistics (subscribers, verification codes, daily metrics)
- **WeChat Configuration**: Token, AppID, AppSecret, EncodingAESKey management with live validation
- **User Management**: Search and view subscriber list with verification history
- **Verification Logs**: Audit trail of all generated codes with status tracking
- **Security Settings**: Password management and WeChat server verification testing
- **System Health**: Memory usage, database connections, uptime monitoring

**Configuration Sync**: Changes made in the admin UI are automatically saved to both the database and `config.toml` file, keeping them in sync. This ensures configuration persists across restarts and can be version-controlled.

**Verification Test**: The "发送验证请求" button in the Security Settings page computes the correct SHA1 signature client-side using your configured token, providing a real end-to-end test of the WeChat verification endpoint.

## Storage Backends

### PostgreSQL

**Pros:**
- Full SQL query capabilities
- ACID transactions
- Better for complex queries and reporting

**Cons:**
- Slower than Redis for simple operations
- Requires schema migrations

**Tables:**
- `wechat_users`: User subscriptions
- `verification_codes`: Generated codes
- `app_config`: Application settings

### Redis

**Pros:**
- Extremely fast for simple operations
- Automatic TTL for verification codes
- No schema migrations needed

**Cons:**
- Limited query capabilities
- Data persistence depends on configuration
- Less suitable for complex reporting

**Data Structures:**
- Hashes: User and code details
- Sorted Sets: Indexing by timestamp
- Strings: Configuration

## Development

### Run Locally

```bash
# Create config file
cp config.toml.example config.toml
# Edit config.toml (set storage.database_url, admin.secret, etc.)

# Run with debug logging
RUST_LOG=debug cargo run
```

### Testing

```bash
# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

### Database Migrations

For PostgreSQL, tables are created automatically on startup:
- `wechat_users`
- `verification_codes`
- `app_config`

For Redis, no setup required.

## Deployment Notes

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

Use Let's Encrypt with Certbot:
```bash
certbot --nginx -d your-domain.com
```

### Firewall

The Docker container maps host port **3317** to container port 3000. If using Nginx reverse proxy, open ports 80/443:
```bash
ufw allow 80/tcp
ufw allow 443/tcp
```

If accessing directly without a reverse proxy:
```bash
ufw allow 3317/tcp
```

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.
