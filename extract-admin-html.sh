#!/bin/bash
# 从 Rust 源代码重新提取 admin.html
# 当修改了 src/admin/ui.rs 中的 HTML 时运行此脚本

set -e

echo "🔄 从 Rust 源代码提取 admin.html..."

# 提取 HTML 内容
sed -n '/^pub const ADMIN_HTML: &str = r#"<!DOCTYPE html>/,/^"#;$/p' \
  src/admin/ui.rs | \
  sed '1s/^pub const ADMIN_HTML: &str = r#//' | \
  sed '$d' | \
  sed '1s/^"<!DOCTYPE html>/<!DOCTYPE html>/' \
  > vercel/public/admin.html

# 验证提取结果
if [ -f "vercel/public/admin.html" ]; then
    lines=$(wc -l < vercel/public/admin.html)
    echo "✅ 成功提取 $lines 行 HTML"
    echo "📝 文件已更新: vercel/public/admin.html"

    # 显示文件开头和结尾
    echo ""
    echo "文件预览:"
    echo "---"
    head -5 vercel/public/admin.html
    echo "..."
    tail -5 vercel/public/admin.html
    echo "---"
else
    echo "❌ 提取失败"
    exit 1
fi
