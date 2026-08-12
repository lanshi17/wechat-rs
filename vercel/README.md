# wechat-rs Vercel 部署指南

本目录包含 wechat-rs 管理后台的 Vercel 前端部署配置。采用**混合架构**：前端部署在 Vercel，后端运行在其他平台（Railway、Render、VPS 等）。

## 🏗️ 架构说明

```
┌─────────────────────────────────────────┐
│  Vercel (前端)                           │
│  ├─ Admin Dashboard (Next.js)            │
│  ├─ CDN 加速静态资源                      │
│  └─ API 路由代理到后端                    │
└─────────────────────────────────────────┘
              ↓ 代理所有 API 请求
┌─────────────────────────────────────────┐
│  后端服务 (Railway/Render/VPS)           │
│  ├─ wechat-rs (Rust Axum 服务器)         │
│  ├─ PostgreSQL / Redis                   │
│  ├─ 后台任务和监控                        │
│  └─ 微信回调处理                          │
└─────────────────────────────────────────┘
```

### 工作原理

1. **前端托管**：Vercel 托管 Next.js 应用，提供全球 CDN 加速
2. **API 代理**：所有 `/admin/*`、`/wx`、`/users` 等请求通过 Next.js rewrites 代理到后端
3. **后端独立**：后端服务独立部署，保持完整的 Rust 性能和功能

## 📋 前提条件

### 1. 部署后端服务

后端需要部署在支持长运行服务的平台。推荐选项：

#### 选项 A：Railway（推荐，最快）

```bash
# 安装 Railway CLI
npm install -g @railway/cli

# 登录
railway login

# 在项目根目录初始化
cd ..  # 回到 wechat-rs 根目录
railway init

# 添加 PostgreSQL
railway add postgres

# 设置环境变量
railway variables set CONFIG_PATH=config.toml
railway variables set RUST_LOG=wechat_rs=info

# 部署
railway up

# 获取后端 URL
railway domain
# 输出类似：https://wechat-rs-production.up.railway.app
```

#### 选项 B：Render

1. 推送代码到 GitHub
2. 访问 [render.com](https://render.com)，创建新 Web Service
3. 连接 GitHub 仓库
4. Render 会自动检测 `Dockerfile` 并构建
5. 添加环境变量：
   - `CONFIG_PATH`: `config.toml`
   - `RUST_LOG`: `wechat_rs=info`
6. 添加 PostgreSQL 数据库
7. 获取后端 URL（类似 `https://wechat-rs.onrender.com`）

#### 选项 C：VPS（阿里云/腾讯云）

```bash
# SSH 到服务器
ssh root@your-server

# 安装 Docker
curl -fsSL https://get.docker.com | sh

# 克隆项目
git clone https://github.com/lanshi17/wechat-rs.git
cd wechat-rs

# 配置
cp config.toml.example config.toml
vim config.toml  # 编辑配置

# 运行
docker-compose up -d

# 后端 URL：http://your-server-ip:3317
```

### 2. 配置后端

确保后端服务：
- ✅ 可以正常访问
- ✅ 已配置好 PostgreSQL/Redis
- ✅ 已设置管理员密码（`admin.password`）
- ✅ 已设置 JWT secret（`admin.secret`，至少 32 位随机字符串）
- ✅ 已配置微信 Token、AppID、AppSecret

## 🚀 部署到 Vercel

### 步骤 1：安装 Vercel CLI

```bash
npm install -g vercel
```

### 步骤 2：登录 Vercel

```bash
vercel login
```

### 步骤 3：设置环境变量

```bash
cd vercel/

# 设置后端 URL（从上面获取的后端地址）
vercel env add BACKEND_URL production
# 输入：https://your-backend-url.up.railway.app

# 可选：设置应用版本号
vercel env add APP_VERSION production
# 输入：0.4.1
```

### 步骤 4：部署

```bash
# 生产环境部署
vercel --prod
```

### 步骤 5：配置自定义域名（可选）

```bash
# 添加自定义域名
vercel domains add admin.yourdomain.com

# 设置为主域名
vercel domains inspect admin.yourdomain.com
```

## 🔄 CI/CD 自动部署

项目配置了 GitHub Actions 工作流，当 `vercel/` 目录下的文件在 `master` 分支上变更时，会自动触发 Vercel 重新部署。

### 配置 GitHub Secrets

在 GitHub 仓库的 Settings → Secrets and variables → Actions 中添加：

| Secret 名称 | 说明 | 获取方式 |
|------------|------|---------|
| `VERCEL_TOKEN` | Vercel API Token | [vercel.com/account/tokens](https://vercel.com/account/tokens) |
| `VERCEL_ORG_ID` | 组织 ID | 在项目 `.vercel/project.json` 中查看 |
| `VERCEL_PROJECT_ID` | 项目 ID | 在项目 `.vercel/project.json` 中查看 |
| `BACKEND_URL` | 后端服务 URL | 你的 Rust 后端服务地址 |

工作流位于 [`.github/workflows/vercel.yml`](../.github/workflows/vercel.yml)，版本号从 `Cargo.toml` 自动读取。

## 🔧 本地开发

```bash
cd vercel/

# 安装依赖
npm install

# 设置环境变量（创建 .env.local）
cat > .env.local << EOF
BACKEND_URL=http://localhost:3317
APP_VERSION=0.4.1-dev
EOF

# 启动开发服务器
npm run dev

# 访问 http://localhost:3000
```

## 📝 环境变量说明

| 变量名 | 必填 | 说明 | 示例 |
|--------|------|------|------|
| `BACKEND_URL` | ✅ | Rust 后端服务地址 | `https://wechat-rs.up.railway.app` |
| `APP_VERSION` | ❌ | 显示在管理后台的版本号 | `0.4.1` |

## 🔍 验证部署

### 1. 检查前端部署

```bash
curl -I https://your-vercel-domain.vercel.app
# 应该返回 200 OK
```

### 2. 检查 API 代理

```bash
# 测试后端连接（通过 Vercel 代理）
curl https://your-vercel-domain.vercel.app/admin/health
# 应该返回后端健康检查数据
```

### 3. 访问管理后台

打开浏览器访问 `https://your-vercel-domain.vercel.app`，使用管理员密码登录。

## ⚠️ 注意事项

### 1. 后端必须支持 HTTPS

微信要求回调地址必须是 HTTPS。如果使用 Railway/Render，它们自动提供 HTTPS。如果使用 VPS，需要配置 SSL 证书（推荐 Let's Encrypt）。

### 2. 微信回调地址配置

在微信公众平台后台配置服务器地址时，使用 Vercel 域名：

```
服务器地址：https://your-vercel-domain.vercel.app/wx
Token：与后端 config.toml 中的 wechat.token 一致
EncodingAESKey：与后端 config.toml 中的 wechat.encoding_aes_key 一致
```

### 3. CORS 配置

由于使用 Next.js rewrites 代理，不存在跨域问题。所有请求都通过 Vercel 代理到后端，对浏览器来说是同源的。

### 4. 性能考虑

- **前端**：Vercel 全球 CDN，访问速度快
- **API 延迟**：每次 API 请求会经过 Vercel → 后端，增加约 50-200ms 延迟
- **推荐区域**：在 `vercel.json` 中设置 `regions` 为靠近后端的区域（如 `hkg1` 香港）

### 5. 监控和日志

- **Vercel 日志**：`vercel logs your-vercel-domain.vercel.app`
- **后端日志**：根据部署平台查看（Railway/Render 都有日志面板）

## 🆚 与直接部署对比

| 方案 | 优点 | 缺点 | 推荐场景 |
|------|------|------|---------|
| **仅后端部署**（Railway/Render） | 简单，一个服务 | 无 CDN 加速 | 快速上线，小规模 |
| **Vercel + 后端混合部署** | CDN 加速，全球访问快 | 需要维护两个服务 | 生产环境，用户分布广 |
| **仅 VPS 部署** | 完全控制 | 需要自己运维 | 有特殊需求 |

## 🐛 故障排查

### 问题 1：API 请求失败

```bash
# 检查后端是否可访问
curl https://your-backend-url.up.railway.app/admin/health

# 检查 Vercel 环境变量
vercel env ls

# 查看 Vercel 日志
vercel logs --follow
```

### 问题 2：微信验证失败

确保：
1. 后端 `wechat.token` 与微信公众平台配置一致
2. 后端 `wechat.encoding_aes_key` 与微信公众平台配置一致
3. 服务器地址使用 Vercel 域名，且可以正常访问

### 问题 3：登录失败

确保：
1. 后端 `admin.password` 已设置（不是默认的 `admin123`）
2. 后端 `admin.secret` 是至少 32 位的随机字符串
3. 浏览器可以正常访问后端 API

## 📚 相关文档

- [主项目 README](../README.md) - 完整的项目文档
- [Vercel 适配分析](../VERCEL_ADAPTATION.md) - 技术架构分析
- [Railway 部署文档](https://docs.railway.app/)
- [Render 部署文档](https://render.com/docs)
- [Vercel 文档](https://vercel.com/docs)

## 💡 优化建议

1. **启用 Vercel Analytics**：监控前端性能
   ```bash
   vercel analytics enable
   ```

2. **配置自定义域名**：使用自己的域名提升品牌形象

3. **启用 HTTPS**：Vercel 自动提供 SSL 证书

4. **监控后端健康**：使用 UptimeRobot 等工具监控后端可用性

5. **定期备份数据库**：根据部署平台提供的备份功能

## 🤝 贡献

如果发现问题或有改进建议，欢迎提交 Issue 或 Pull Request！
