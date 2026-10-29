# Railway 部署指南

本文档说明如何将 wechat-rs 后端部署到 [Railway](https://railway.app)。

## 📋 前提条件

- Railway 账号（[注册](https://railway.app)）
- Railway CLI（`npm install -g @railway/cli`）
- 微信公众平台的 Token、AppID、AppSecret

## 🚀 部署步骤

### 步骤 1：克隆并配置

```bash
git clone https://github.com/lanshi17/wechat-rs.git
cd wechat-rs
```

### 步骤 2：登录 Railway

```bash
railway login
```

### 步骤 3：初始化项目

```bash
railway init
# 选择创建新项目或关联到现有项目
```

### 步骤 4：添加 PostgreSQL 数据库

```bash
railway add
# 选择 "Database" → "PostgreSQL"
```

Railway 会自动设置 `DATABASE_URL` 环境变量，wechat-rs 会自动检测并使用它。

### 步骤 5：配置环境变量

```bash
# 必填：管理员密码（至少 8 位）
railway variables set ADMIN_PASSWORD=your_secure_password

# 必填：JWT 签名密钥（至少 32 位随机字符串）
railway variables set ADMIN_SECRET=$(openssl rand -base64 48)

# 微信配置（可选，也可在管理后台配置）
railway variables set WECHAT_TOKEN=your_wechat_token
railway variables set WECHAT_APPID=your_appid
railway variables set WECHAT_APPSECRET=your_appsecret
railway variables set WECHAT_ENCODING_AES_KEY=your_encoding_aes_key

# 上游服务令牌（用于 /api/wechat/user 接口验证）
railway variables set UPSTREAM_SERVER_TOKEN=$(openssl rand -base64 32)
```

### 步骤 6：部署

```bash
railway up
```

### 步骤 7：获取域名

```bash
railway domain
# 输出类似：https://wechat-rs-production.up.railway.app
```

## 🔧 环境变量参考

| 变量名 | 必填 | 说明 |
|--------|------|------|
| `ADMIN_PASSWORD` | ✅ | 管理员初始密码（首次登录后存入数据库） |
| `ADMIN_SECRET` | ✅ | JWT 签名密钥（至少 32 位） |
| `WECHAT_TOKEN` | ❌ | 微信验证 Token（可在管理后台配置） |
| `WECHAT_APPID` | ❌ | 微信 AppID |
| `WECHAT_APPSECRET` | ❌ | 微信 AppSecret |
| `WECHAT_ENCODING_AES_KEY` | ❌ | 微信消息加解密密钥 |
| `UPSTREAM_SERVER_TOKEN` | ❌ | 上游 API 访问令牌 |
| `DATABASE_URL` | 自动 | Railway PostgreSQL 自动注入 |
| `PORT` | 自动 | Railway 自动注入 |
| `CONFIG_PATH` | ❌ | 配置文件路径（默认为 `config.toml`） |

## 🌐 配置微信公众平台

部署完成后，在微信公众平台后台配置：

- **服务器地址（URL）**：`https://your-railway-domain.up.railway.app/wx`
- **令牌（Token）**：与 `WECHAT_TOKEN` 一致
- **消息加解密密钥**：与 `WECHAT_ENCODING_AES_KEY` 一致
- **消息加密方式**：安全模式（推荐）

## 📊 查看日志

```bash
# 实时日志
railway logs

# 指定服务日志
railway logs --service wechat-rs
```

## 💰 费用说明

Railway 免费套餐包含：
- $5 额度/月
- 500 小时运行时间
- 1 GB 数据库存储

对于个人项目，免费额度通常足够。生产环境建议升级到付费套餐。

## 🔍 验证部署

```bash
# 检查健康状态（公开端点，无需鉴权）
curl https://your-railway-domain.up.railway.app/api/monitor/status

# 应该返回 JSON 包含 status: "ok"、uptime_secs、system 等字段
# （/admin/health 需要管理员鉴权，仅用于登录后的管理后台）
```

## 🆕 更新部署

```bash
# 推送代码后
git push origin master
railway up
```

Railway 会自动检测新提交并重新构建。

## 🐛 常见问题

### 1. 启动失败

```bash
# 查看日志定位问题
railway logs
```

常见原因：
- `ADMIN_SECRET` 太短（需要至少 32 位）
- 使用默认密码 `admin123` 且数据库中没有已存在的密码哈希
- PostgreSQL 连接失败

### 2. 微信验证失败

确保：
- `WECHAT_TOKEN` 与微信公众平台配置一致
- 服务器地址使用 HTTPS（Railway 自动提供）
- 编码 AES Key 为 43 位字符串

### 3. 数据库连接问题

Railway 自动注入 `DATABASE_URL`，无需手动配置。如需手动配置：

```bash
railway variables set DATABASE_URL=postgres://user:pass@host:5432/dbname
```

## 📚 相关文档

- [主 README](README.md)
- [Railway 官方文档](https://docs.railway.app)
