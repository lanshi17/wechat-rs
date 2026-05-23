# wechat-rs

A high-performance WeChat Official Account backend service written in Rust.

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
├── main.rs              # Application entry point, routing, core handlers
├── admin.rs             # Admin API and web UI
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
# Build and start
docker-compose up -d --build

# View logs
docker-compose logs -f

# Stop
docker-compose down
```

## Configuration

Environment variables (via `.env` file or system environment):

### Required

```bash
# Storage backend: "postgres" or "redis"
STORAGE_TYPE=postgres

# For PostgreSQL
DATABASE_URL=postgres://user:password@host:5432/dbname

# For Redis
REDIS_URL=redis://host:6379

# Admin credentials
ADMIN_PASSWORD=your-secure-password
ADMIN_SECRET=your-jwt-secret-at-least-32-chars

# WeChat Official Account credentials
WECHAT_TOKEN=your-wechat-token
WECHAT_APPID=your-appid
WECHAT_APPSECRET=your-appsecret
WECHAT_ENCODING_AES_KEY=your-43-character-key
```

### Optional

```bash
# Service configuration
LISTEN_ADDR=0.0.0.0:3000
SITE_NAME=微信服务管理后台
DOMAIN=your-domain.com

# Upstream API token (for /api/wechat/user endpoint)
WECHAT_SERVER_TOKEN=your-upstream-token

# Logging
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
http://your-domain:3000/admin/
```

Features:
- Dashboard with real-time statistics
- User management with search
- Verification code audit logs
- Site configuration
- System health monitoring
- Password management

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
# Set environment variables
export DATABASE_URL=postgres://localhost:5432/wechat
export ADMIN_PASSWORD=dev-password
export ADMIN_SECRET=dev-secret-min-32-chars
export WECHAT_TOKEN=dev-token

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
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

### HTTPS

Use Let's Encrypt with Certbot:
```bash
certbot --nginx -d your-domain.com
```

### Firewall

Open port 80/443 for WeChat callbacks:
```bash
ufw allow 80/tcp
ufw allow 443/tcp
```

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.
