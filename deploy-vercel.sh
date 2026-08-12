#!/bin/bash
set -e

echo "🚀 wechat-rs Vercel 部署脚本"
echo "================================"
echo ""

# 检查是否在项目根目录
if [ ! -f "Cargo.toml" ]; then
    echo "❌ 错误：请在 wechat-rs 项目根目录运行此脚本"
    exit 1
fi

# 检查后端是否已部署
echo "📋 步骤 1: 确认后端部署"
echo ""
read -p "后端是否已部署到 Railway/Render/VPS? (y/n) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo ""
    echo "请先部署后端服务。推荐方案："
    echo ""
    echo "Railway（最快）："
    echo "  railway init"
    echo "  railway add   # 选择 Database → PostgreSQL"
    echo "  railway variables set ADMIN_PASSWORD=your_password"
    echo "  railway variables set ADMIN_SECRET=\$(openssl rand -base64 48)"
    echo "  railway variables set WECHAT_TOKEN=your_token"
    echo "  railway variables set WECHAT_APPID=wx..."
    echo "  railway variables set WECHAT_APPSECRET=..."
    echo "  railway up"
    echo "  railway domain  # 记录这个 URL"
    echo ""
    echo "  详细说明请查看 RAILWAY.md"
    echo ""
    echo "Render："
    echo "  1. 推送代码到 GitHub"
    echo "  2. 在 render.com 创建 Web Service"
    echo "  3. 连接仓库，自动构建"
    echo ""
    exit 1
fi

# 获取后端 URL
echo ""
read -p "请输入后端 URL (例如 https://wechat-rs.up.railway.app): " BACKEND_URL
if [ -z "$BACKEND_URL" ]; then
    echo "❌ 错误：后端 URL 不能为空"
    exit 1
fi

# 进入 vercel 目录
cd vercel/

# 检查是否已安装依赖
if [ ! -d "node_modules" ]; then
    echo ""
    echo "📦 步骤 2: 安装依赖"
    npm install
else
    echo ""
    echo "✅ 依赖已安装"
fi

# 检查 Vercel CLI
if ! command -v vercel &> /dev/null; then
    echo ""
    echo "📦 安装 Vercel CLI"
    npm install -g vercel
fi

# 设置环境变量
echo ""
echo "⚙️  步骤 3: 配置环境变量"
vercel env add BACKEND_URL production <<< "$BACKEND_URL" 2>/dev/null || echo "环境变量已存在"

# 读取版本号
VERSION=$(grep "^version" ../Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "检测到版本号: $VERSION"
vercel env add APP_VERSION production <<< "$VERSION" 2>/dev/null || echo "环境变量已存在"

# 部署
echo ""
echo "🚀 步骤 4: 部署到 Vercel"
read -p "是否继续部署到生产环境? (y/n) " -n 1 -r
echo ""
if [[ $REPLY =~ ^[Yy]$ ]]; then
    vercel --prod
    echo ""
    echo "✅ 部署完成！"
    echo ""
    echo "📝 下一步："
    echo "1. 访问 Vercel 提供的域名"
    echo "2. 使用管理员密码登录"
    echo "3. 在微信公众平台配置服务器地址为：https://your-vercel-domain.vercel.app/wx"
else
    echo ""
    echo "部署已取消"
    echo "稍后可以运行: cd vercel && vercel --prod"
fi
