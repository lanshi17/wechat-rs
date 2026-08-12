# Vercel 适配 - 快速参考

## 📦 创建的文件

### Vercel 前端项目 (`vercel/`)
- `package.json` - Node.js 依赖配置
- `next.config.js` - Next.js 配置，包含 API 代理规则
- `vercel.json` - Vercel 部署配置
- `tsconfig.json` - TypeScript 配置
- `app/layout.tsx` - Next.js 布局组件
- `app/page.tsx` - 主页，动态加载 admin.html
- `public/admin.html` - 管理后台 HTML（从 Rust 提取）
- `README.md` - 详细部署文档
- `.env.example` - 环境变量示例
- `.gitignore` - Node.js gitignore

### 部署工具
- `deploy-vercel.sh` - 自动化部署脚本
- `extract-admin-html.sh` - HTML 提取脚本（同步 Rust 代码变更）

### 文档
- `VERCEL_ADAPTATION.md` - 技术分析（为什么不能直接部署）
- `VERCEL_SUMMARY.md` - 完整工作总结
- `README.md` - 已更新，添加 Vercel 章节

## 🚀 快速开始

### 方式 1：自动部署（推荐）

```bash
./deploy-vercel.sh
```

### 方式 2：手动部署

```bash
# 1. 部署后端（Railway 示例）
railway init
railway add postgres
railway variables set CONFIG_PATH=config.toml
railway up
railway domain  # 记录 URL

# 2. 部署前端
cd vercel/
npm install
vercel env add BACKEND_URL production
vercel --prod
```

### 方式 3：本地开发

```bash
cd vercel/
npm install
echo "BACKEND_URL=http://localhost:3317" > .env.local
npm run dev
# 访问 http://localhost:3000
```

## 🏗️ 架构

```
Vercel (前端) ──proxy──▶ Backend (Railway/Render/VPS)
  ├─ Admin Dashboard         ├─ Rust Axum Server
  ├─ CDN 加速                ├─ PostgreSQL/Redis
  └─ API 代理                └─ 微信回调处理
```

## 📝 环境变量

| 变量 | 必填 | 说明 |
|------|------|------|
| `BACKEND_URL` | ✅ | Rust 后端地址 |
| `APP_VERSION` | ❌ | 版本号显示 |

## 🔧 维护

### 同步 HTML 变更

如果修改了 `src/admin/ui.rs`：

```bash
./extract-admin-html.sh
```

### 验证构建

```bash
cd vercel/
BACKEND_URL=http://localhost:3317 npm run build
```

## 📚 详细文档

- [vercel/README.md](vercel/README.md) - 完整部署指南
- [VERCEL_ADAPTATION.md](VERCEL_ADAPTATION.md) - 技术分析
- [VERCEL_SUMMARY.md](VERCEL_SUMMARY.md) - 工作总结

## ✅ 验证清单

- [x] 所有文件已创建
- [x] 构建测试通过
- [x] 文档已更新
- [x] 脚本可执行
- [x] HTML 提取正确

## 💡 提示

1. **后端必须先部署**：Vercel 前端依赖后端 API
2. **使用 HTTPS**：微信要求回调地址必须是 HTTPS
3. **配置微信回调**：使用 Vercel 域名 `https://your-domain.vercel.app/wx`
4. **监控成本**：Vercel 免费额度足够小规模使用

## 🆘 常见问题

**Q: 为什么不能直接部署到 Vercel？**
A: Rust 后端是长运行服务器，Vercel 只支持无服务器函数。详见 [VERCEL_ADAPTATION.md](VERCEL_ADAPTATION.md)。

**Q: 必须用 Vercel 吗？**
A: 不是。可以直接部署后端到 Railway/Render，或使用 VPS。Vercel 是可选的前端加速方案。

**Q: 如何更新管理后台界面？**
A: 修改 `src/admin/ui.rs`，然后运行 `./extract-admin-html.sh` 同步到 Vercel。

**Q: API 延迟会增加吗？**
A: 会。每次请求经过 Vercel 代理，增加约 50-200ms。对于管理后台来说可以接受。
