//! 管理后台 HTML 模板

pub const ADMIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title id="page-title">微信服务管理后台</title>
<style>
  @import url('https://fonts.googleapis.com/css2?family=Noto+Serif+SC:wght@400;600&family=JetBrains+Mono:wght@400;500&display=swap');

  :root {
    --ink:     #1a1814;
    --ink2:    #4a453f;
    --ink3:    #8a837a;
    --paper:   #f7f4ef;
    --paper2:  #ede9e1;
    --paper3:  #e0dbd0;
    --red:     #c0392b;
    --red-l:   #fdf0ee;
    --green:   #2d6a4f;
    --green-l: #edf7f1;
    --gold:    #8b6914;
    --gold-l:  #fdf8ec;
    --border:  #cec8be;
    --r:       6px;
    --shadow:  0 2px 12px rgba(0,0,0,0.08);
  }

  * { box-sizing: border-box; margin: 0; padding: 0; }

  body {
    font-family: 'Noto Serif SC', serif;
    background: var(--paper);
    color: var(--ink);
    min-height: 100vh;
    display: flex;
  }

  /* ── 侧栏 ── */
  aside {
    width: 220px;
    flex-shrink: 0;
    background: var(--ink);
    color: var(--paper);
    display: flex;
    flex-direction: column;
    padding: 32px 0 24px;
    position: fixed;
    top: 0; bottom: 0; left: 0;
  }
  .brand {
    padding: 0 24px 28px;
    border-bottom: 1px solid rgba(255,255,255,0.1);
  }
  .brand h1 { font-size: 18px; font-weight: 600; line-height: 1.3; }
  .brand p  { font-size: 11px; color: rgba(255,255,255,0.4); margin-top: 4px; font-family: 'JetBrains Mono', monospace; }

  nav { flex: 1; padding: 16px 0; }
  nav a {
    display: flex; align-items: center; gap: 10px;
    padding: 11px 24px; font-size: 14px;
    color: rgba(255,255,255,0.65);
    text-decoration: none; cursor: pointer;
    border-left: 3px solid transparent;
    transition: all 0.15s;
  }
  nav a:hover  { color: #fff; background: rgba(255,255,255,0.06); }
  nav a.active { color: #fff; border-left-color: #c9a84c; background: rgba(255,255,255,0.08); }
  nav a .ico   { font-size: 17px; width: 20px; text-align: center; }

  .aside-footer {
    padding: 16px 24px 0;
    border-top: 1px solid rgba(255,255,255,0.1);
    font-size: 11px;
    color: rgba(255,255,255,0.3);
    font-family: 'JetBrains Mono', monospace;
  }

  /* ── 主区 ── */
  main {
    margin-left: 220px;
    flex: 1;
    padding: 40px 48px;
    max-width: 960px;
  }

  /* ── 登录页 ── */
  #login-page {
    position: fixed; inset: 0;
    background: var(--paper);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .login-box {
    width: 360px;
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 40px;
    box-shadow: var(--shadow);
    text-align: center;
  }
  .login-box h2 { font-size: 22px; margin-bottom: 4px; }
  .login-box p  { font-size: 13px; color: var(--ink3); margin-bottom: 28px; }

  /* ── 面板标题 ── */
  .page-header { margin-bottom: 28px; }
  .page-header h2 { font-size: 22px; font-weight: 600; }
  .page-header p  { font-size: 13px; color: var(--ink3); margin-top: 4px; }

  /* ── 统计卡片 ── */
  .stats-row { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 32px; }
  .stat-card {
    background: #fff; border: 1px solid var(--border);
    border-radius: var(--r); padding: 20px 24px;
  }
  .stat-card .label { font-size: 12px; color: var(--ink3); margin-bottom: 8px; }
  .stat-card .value { font-size: 32px; font-weight: 600; font-family: 'JetBrains Mono', monospace; }
  .stat-card .sub   { font-size: 11px; color: var(--ink3); margin-top: 4px; }

  /* ── 表单 ── */
  .card {
    background: #fff; border: 1px solid var(--border);
    border-radius: var(--r); padding: 28px 32px;
    margin-bottom: 20px;
  }
  .card h3 { font-size: 15px; font-weight: 600; margin-bottom: 20px; padding-bottom: 12px; border-bottom: 1px solid var(--paper3); }

  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .form-grid.full { grid-template-columns: 1fr; }

  .field { display: flex; flex-direction: column; gap: 6px; }
  .field label { font-size: 12px; color: var(--ink2); font-weight: 600; letter-spacing: 0.03em; }
  .field input, .field textarea {
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 9px 12px;
    font-size: 14px;
    font-family: inherit;
    color: var(--ink);
    background: var(--paper);
    transition: border-color 0.15s, background 0.15s;
    outline: none;
  }
  .field input:focus, .field textarea:focus {
    border-color: var(--ink);
    background: #fff;
  }
  .field .hint { font-size: 11px; color: var(--ink3); }

  /* ── 按钮 ── */
  .btn {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 9px 20px; font-size: 14px; font-family: inherit;
    border-radius: var(--r); cursor: pointer;
    border: 1px solid transparent; transition: all 0.15s;
    font-weight: 600;
  }
  .btn-primary { background: var(--ink); color: #fff; }
  .btn-primary:hover { background: #333; }
  .btn-danger  { background: var(--red-l); color: var(--red); border-color: #f5c6c2; }
  .btn-danger:hover  { background: #fde0dc; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }

  /* ── 用户表格 ── */
  .table-wrap { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; padding: 10px 14px; font-size: 11px; font-weight: 600;
       color: var(--ink3); border-bottom: 2px solid var(--paper3); letter-spacing: 0.05em; }
  td { padding: 12px 14px; border-bottom: 1px solid var(--paper2); }
  tr:last-child td { border-bottom: none; }
  tr:hover td { background: var(--paper); }
  .badge {
    display: inline-block; padding: 2px 8px; border-radius: 20px;
    font-size: 11px; font-weight: 600;
  }
  .badge-on  { background: var(--green-l); color: var(--green); }
  .badge-off { background: var(--red-l);   color: var(--red); }
  .openid-cell { font-family: 'JetBrains Mono', monospace; font-size: 11px; color: var(--ink3); }

  /* ── 分页 ── */
  .pagination { display: flex; align-items: center; gap: 8px; margin-top: 16px; justify-content: flex-end; }
  .pagination button {
    padding: 5px 14px; border: 1px solid var(--border); border-radius: var(--r);
    background: #fff; cursor: pointer; font-size: 13px; font-family: inherit;
  }
  .pagination button:disabled { opacity: 0.4; cursor: not-allowed; }
  .pagination span { font-size: 12px; color: var(--ink3); }

  /* ── Toast ── */
  #toast {
    position: fixed; bottom: 32px; right: 32px;
    padding: 12px 20px; border-radius: var(--r);
    font-size: 13px; font-weight: 600;
    box-shadow: 0 4px 20px rgba(0,0,0,0.15);
    transition: opacity 0.3s; opacity: 0;
    z-index: 999; pointer-events: none;
  }
  #toast.show { opacity: 1; }
  #toast.ok  { background: var(--green-l); color: var(--green); }
  #toast.err { background: var(--red-l);   color: var(--red); }

  /* ── 分隔 ── */
  .section-gap { height: 32px; }

  .mono { font-family: 'JetBrains Mono', monospace; }

  /* ── 页面切换 ── */
  .page { display: none; }
  .page.active { display: block; }
</style>
</head>
<body>

<!-- 登录页 -->
<div id="login-page">
  <div class="login-box">
    <h2 id="login-title">微信服务管理后台</h2>
    <p id="login-subtitle">管理员登录</p>
    <div class="field" style="margin-bottom:16px;text-align:left">
      <label>管理员密码</label>
      <input type="password" id="pw-input" placeholder="请输入密码" />
    </div>
    <button class="btn btn-primary" id="login-btn" onclick="doLogin()" style="width:100%;justify-content:center">登 录</button>
    <p id="login-err" style="color:#c0392b;font-size:12px;margin-top:12px;min-height:16px"></p>
  </div>
</div>

<!-- 侧栏 -->
<aside>
  <div class="brand">
    <h1 id="brand-title">微信服务管理后台</h1>
    <p id="brand-domain">—</p>
  </div>
  <nav>
    <a class="active" onclick="showPage('overview')" id="nav-overview">
      <span class="ico">◈</span> 概览
    </a>
    <a onclick="showPage('config')" id="nav-config">
      <span class="ico">⚙</span> 微信配置
    </a>
    <a onclick="showPage('users')" id="nav-users">
      <span class="ico">◎</span> 用户管理
    </a>
    <a onclick="showPage('codes')" id="nav-codes">
      <span class="ico">▤</span> 验证日志
    </a>
    <a onclick="showPage('security')" id="nav-security">
      <span class="ico">◇</span> 安全设置
    </a>
    <a onclick="showPage('health')" id="nav-health">
      <span class="ico">◈</span> 系统状态
    </a>
  </nav>
  <div class="aside-footer">v0.3.0 · PostgreSQL</div>
</aside>

<!-- 主区域 -->
<main>

  <!-- 概览 -->
  <div class="page active" id="page-overview">
    <div class="page-header">
      <h2>数据概览</h2>
      <p>公众号实时订阅数据</p>
    </div>
    <div class="stats-row" style="grid-template-columns: repeat(4, 1fr)">
      <div class="stat-card">
        <div class="label">当前关注人数</div>
        <div class="value" id="stat-total">—</div>
        <div class="sub">活跃订阅</div>
      </div>
      <div class="stat-card">
        <div class="label">今日新增关注</div>
        <div class="value" id="stat-today-new">—</div>
        <div class="sub">当日数据</div>
      </div>
      <div class="stat-card">
        <div class="label">今日验证码</div>
        <div class="value" id="stat-today-codes">—</div>
        <div class="sub">当日生成</div>
      </div>
      <div class="stat-card">
        <div class="label">验证码总计</div>
        <div class="value" id="stat-total-codes">—</div>
        <div class="sub"><span id="stat-used-codes">0</span> 已使用 · <span id="stat-expired-codes">0</span> 已过期</div>
      </div>
    </div>
    <div class="card">
      <h3>快速入门</h3>
      <div style="font-size:13px;color:var(--ink2);line-height:2">
        <p>① 在「微信配置」中填写 Token、AppID、AppSecret 并保存</p>
        <p>② 在微信公众平台后台将服务器地址配置为 <code class="mono" style="background:var(--paper2);padding:2px 6px;border-radius:4px" id="guide-url">https://your-domain.com/wx</code></p>
        <p>③ 用户关注后将自动写入 wechat_users 表，可在「用户管理」中查看</p>
      </div>
    </div>
  </div>

  <!-- 微信配置 -->
  <div class="page" id="page-config">
    <div class="page-header">
      <h2>微信配置</h2>
      <p>公众号接入参数，保存后实时生效，无需重启服务</p>
    </div>
    <div class="card">
      <h3>站点设置</h3>
      <div class="form-grid">
        <div class="field">
          <label>站点名称</label>
          <input type="text" id="cfg-site-name" placeholder="微信服务管理后台" />
          <span class="hint">显示在侧栏和浏览器标签页</span>
        </div>
        <div class="field">
          <label>自定义域名</label>
          <input type="text" id="cfg-domain" placeholder="auth.example.com" class="mono" />
          <span class="hint">用于展示，不影响实际路由</span>
        </div>
      </div>
    </div>
    <div class="card">
      <h3>接入配置</h3>
      <div class="form-grid">
        <div class="field">
          <label>Token（令牌）</label>
          <input type="text" id="cfg-token" placeholder="与公众平台后台保持一致" />
          <span class="hint">用于验证消息来自微信服务器</span>
        </div>
        <div class="field">
          <label>AppID</label>
          <input type="text" id="cfg-appid" placeholder="wx..." class="mono" />
        </div>
        <div class="field">
          <label>AppSecret</label>
          <input type="password" id="cfg-appsecret" placeholder="留空表示不修改" />
          <span class="hint">当前值已脱敏显示，重新填写即可覆盖</span>
        </div>
        <div class="field">
          <label>AppSecret 当前值（脱敏）</label>
          <input type="text" id="cfg-appsecret-masked" readonly style="cursor:default;color:var(--ink3)" />
        </div>
        <div class="field">
          <label>EncodingAESKey（消息加解密密钥）</label>
          <input type="password" id="cfg-aeskey" placeholder="43位字符串，留空表示不修改" />
          <span class="hint">当前值已脱敏显示，重新填写即可覆盖</span>
        </div>
        <div class="field">
          <label>EncodingAESKey 当前值（脱敏）</label>
          <input type="text" id="cfg-aeskey-masked" readonly style="cursor:default;color:var(--ink3)" />
        </div>
      </div>
    </div>
    <div class="card">
      <h3>消息设置</h3>
      <div class="form-grid full">
        <div class="field">
          <label>关注欢迎语</label>
          <textarea id="cfg-welcome" rows="3" placeholder="感谢关注！"></textarea>
        </div>
      </div>
    </div>
    <button class="btn btn-primary" onclick="saveConfig()">保存配置</button>

    <div class="card" style="margin-top:20px">
      <h3>自定义菜单</h3>
      <p style="font-size:13px;color:var(--ink2);margin-bottom:16px">
        点击按钮将在公众号底部创建「获取验证码」菜单。用户点击后自动生成 6 位验证码并回复。<br/>
        <span style="color:var(--ink3)">需先保存正确的 AppID 和 AppSecret，菜单创建后约 1 分钟生效（或取消关注后重新关注立即生效）。</span>
      </p>
      <button class="btn btn-primary" onclick="createMenu()" id="create-menu-btn">创建菜单</button>
      <div id="menu-result" style="margin-top:12px;font-size:13px;min-height:20px"></div>
    </div>
  </div>

  <!-- 用户管理 -->
  <div class="page" id="page-users">
    <div class="page-header">
      <h2>用户管理</h2>
      <p>已关注用户列表，取关用户已软删除（subscribe=false）</p>
    </div>
    <div class="card" style="padding:12px 16px;margin-bottom:16px">
      <div style="display:flex;gap:12px;align-items:center">
        <input type="text" id="user-search-input" placeholder="输入 OpenID 搜索用户…" style="flex:1;border:1px solid var(--border);border-radius:var(--r);padding:8px 12px;font-size:13px;font-family:'JetBrains Mono',monospace;background:var(--paper);outline:none" />
        <button class="btn btn-primary" onclick="searchUsers()" style="padding:8px 16px;font-size:13px">搜索</button>
        <button class="btn" onclick="clearSearch()" style="padding:8px 16px;font-size:13px;background:var(--paper2);color:var(--ink2)">清除</button>
      </div>
    </div>
    <div class="card" style="padding:0">
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>OpenID</th>
              <th>昵称</th>
              <th>状态</th>
              <th>首次关注</th>
              <th>最近变更</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody id="user-tbody">
            <tr><td colspan="6" style="text-align:center;color:var(--ink3);padding:32px">加载中…</td></tr>
          </tbody>
        </table>
      </div>
      <div class="pagination" style="padding:12px 16px">
        <button id="prev-btn" onclick="prevPage()" disabled>上一页</button>
        <span id="page-info">第 1 页</span>
        <button id="next-btn" onclick="nextPage()">下一页</button>
      </div>
    </div>
  </div>

  <!-- 验证日志 -->
  <div class="page" id="page-codes">
    <div class="page-header">
      <h2>验证日志</h2>
      <p>所有验证码生成记录，包含已使用、未使用和已过期的验证码</p>
    </div>
    <div class="card" style="padding:0">
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>OpenID</th>
              <th>验证码</th>
              <th>状态</th>
              <th>创建时间</th>
              <th>过期时间</th>
            </tr>
          </thead>
          <tbody id="codes-tbody">
            <tr><td colspan="6" style="text-align:center;color:var(--ink3);padding:32px">加载中…</td></tr>
          </tbody>
        </table>
      </div>
      <div class="pagination" style="padding:12px 16px">
        <button id="codes-prev-btn" onclick="codesPrevPage()" disabled>上一页</button>
        <span id="codes-page-info">第 1 页</span>
        <button id="codes-next-btn" onclick="codesNextPage()">下一页</button>
        <span style="margin-left:12px;color:var(--ink3);font-size:12px" id="codes-total-info">共 0 条</span>
      </div>
    </div>
  </div>

  <!-- 安全设置 -->
  <div class="page" id="page-security">
    <div class="page-header">
      <h2>安全设置</h2>
      <p>修改管理员登录密码与接口验证</p>
    </div>
    <div class="card">
      <h3>修改密码</h3>
      <div class="form-grid">
        <div class="field">
          <label>新密码</label>
          <input type="password" id="new-pw" placeholder="至少 8 位" />
        </div>
        <div class="field">
          <label>确认新密码</label>
          <input type="password" id="new-pw2" placeholder="再次输入" />
        </div>
      </div>
      <div style="margin-top:16px">
        <button class="btn btn-primary" onclick="changePw()">更新密码</button>
        <button class="btn btn-danger" onclick="logout()" style="margin-left:12px">退出登录</button>
      </div>
    </div>

    <div class="card">
      <h3>接口验证</h3>
      <p style="font-size:13px;color:var(--ink2);margin-bottom:16px">
        测试微信服务器验证接口（GET /wx），验证 Token 配置是否正确。
      </p>
      <button class="btn btn-primary" onclick="testVerification()">发送验证请求</button>
      <div id="verify-result" style="margin-top:16px;padding:12px;background:var(--paper);border-radius:var(--r);display:none">
        <pre style="font-size:12px;color:var(--ink2);white-space:pre-wrap;margin:0"></pre>
      </div>
    </div>
  </div>

  <!-- 系统状态 -->
  <div class="page" id="page-health">
    <div class="page-header">
      <h2>系统状态</h2>
      <p>服务器运行状态与资源监控</p>
    </div>
    <div class="stats-row">
      <div class="stat-card">
        <div class="label">运行时间</div>
        <div class="value mono" style="font-size:14px;margin-top:8px" id="health-uptime">—</div>
      </div>
      <div class="stat-card">
        <div class="label">内存使用</div>
        <div class="value mono" style="font-size:18px;margin-top:8px" id="health-memory">—</div>
        <div class="sub" id="health-memory-detail">—</div>
      </div>
      <div class="stat-card">
        <div class="label">数据库状态</div>
        <div class="value" style="font-size:18px;margin-top:8px" id="health-db">—</div>
        <div class="sub" id="health-db-detail">—</div>
      </div>
    </div>
    <div class="card">
      <h3>服务信息</h3>
      <div style="font-size:13px;color:var(--ink2);line-height:2.2">
        <div style="display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid var(--paper2)">
          <span style="color:var(--ink3)">服务版本</span><span class="mono">v0.3.0</span>
        </div>
        <div style="display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid var(--paper2)">
          <span style="color:var(--ink3)">监听端口</span><span class="mono">3317</span>
        </div>
        <div style="display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid var(--paper2)">
          <span style="color:var(--ink3)">域名</span><span class="mono" id="health-domain">—</span>
        </div>
        <div style="display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid var(--paper2)">
          <span style="color:var(--ink3)">数据库</span><span class="mono" id="health-db-addr">PostgreSQL</span>
        </div>
        <div style="display:flex;justify-content:space-between;padding:4px 0">
          <span style="color:var(--ink3)">数据库连接池</span><span class="mono" id="health-db-conns">—</span>
        </div>
      </div>
    </div>
    <button class="btn btn-primary" onclick="loadHealth()" style="margin-top:8px">刷新状态</button>
  </div>

</main>

<div id="toast"></div>

<script>
const BASE = '';
let token = localStorage.getItem('admin_token') || '';
let curPage = 1;
const pageSize = 20;
let codePage = 1;
const codePageSize = 20;

// ── 启动 ──────────────────────────────────────────────────────────────────────
window.onload = async () => {
  if (token) {
    const ok = await testToken();
    if (ok) { showApp(); return; }
    token = '';
    localStorage.removeItem('admin_token');
  }
  document.getElementById('login-page').style.display = 'flex';
  document.getElementById('pw-input').addEventListener('keydown', e => { if(e.key==='Enter') doLogin(); });
};

async function testToken() {
  const r = await fetch(BASE+'/admin/stats', { headers: authH() });
  return r.ok;
}

function showApp() {
  document.getElementById('login-page').style.display = 'none';
  loadStats();
  loadDetailedStats();
  loadConfig();
  const searchInput = document.getElementById('user-search-input');
  if (searchInput) searchInput.addEventListener('keydown', e => { if(e.key==='Enter') searchUsers(); });
}

// ── 登录 ──────────────────────────────────────────────────────────────────────
async function doLogin() {
  const pw = document.getElementById('pw-input').value;
  document.getElementById('login-btn').disabled = true;
  const r = await fetch(BASE+'/admin/login', {
    method: 'POST',
    headers: {'Content-Type':'application/json'},
    body: JSON.stringify({password: pw}),
  });
  document.getElementById('login-btn').disabled = false;
  if (r.ok) {
    const d = await r.json();
    token = d.token;
    localStorage.setItem('admin_token', token);
    showApp();
  } else {
    document.getElementById('login-err').textContent = '密码错误，请重试';
  }
}

function logout() {
  token = '';
  localStorage.removeItem('admin_token');
  location.reload();
}

// ── 导航 ──────────────────────────────────────────────────────────────────────
function showPage(name) {
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.querySelectorAll('nav a').forEach(a => a.classList.remove('active'));
  document.getElementById('page-'+name).classList.add('active');
  document.getElementById('nav-'+name).classList.add('active');
  if (name === 'users') { curPage = 1; loadUsers(); }
  if (name === 'codes') { codePage = 1; loadCodes(); }
  if (name === 'health') { loadHealth(); }
  if (name === 'overview') { loadDetailedStats(); }
}

// ── 数据加载 ──────────────────────────────────────────────────────────────────
async function loadStats() {
  const r = await apiFetch('/admin/stats');
  if (!r) return;
  document.getElementById('stat-total').textContent = r.total_subscribers.toLocaleString();
}

async function loadDetailedStats() {
  const r = await apiFetch('/admin/stats/detailed');
  if (!r) return;
  document.getElementById('stat-total').textContent = r.total_subscribers.toLocaleString();
  document.getElementById('stat-today-new').textContent = r.today_new_users.toLocaleString();
  document.getElementById('stat-today-codes').textContent = r.today_codes.toLocaleString();
  document.getElementById('stat-total-codes').textContent = r.total_codes.toLocaleString();
  document.getElementById('stat-used-codes').textContent = r.used_codes.toLocaleString();
  document.getElementById('stat-expired-codes').textContent = r.expired_codes.toLocaleString();
}

async function loadCodes() {
  const r = await apiFetch(`/admin/codes?page=${codePage}&size=${codePageSize}`);
  const tbody = document.getElementById('codes-tbody');
  if (!r || !r.codes || !r.codes.length) {
    tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--ink3);padding:32px">暂无数据</td></tr>';
    document.getElementById('codes-next-btn').disabled = true;
    document.getElementById('codes-total-info').textContent = `共 ${r ? r.total : 0} 条`;
    return;
  }
  const now = new Date();
  tbody.innerHTML = r.codes.map(c => {
    let badge;
    if (c.used) badge = '<span class="badge badge-on">已使用</span>';
    else if (new Date(c.expires_at) < now) badge = '<span class="badge badge-off">已过期</span>';
    else badge = '<span class="badge" style="background:var(--gold-l);color:var(--gold)">有效</span>';
    return `<tr>
      <td style="color:var(--ink3)">${c.id}</td>
      <td class="openid-cell">${c.openid}</td>
      <td class="mono" style="font-weight:600;letter-spacing:2px">${c.code}</td>
      <td>${badge}</td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(c.created_at)}</td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(c.expires_at)}</td>
    </tr>`;
  }).join('');
  document.getElementById('codes-prev-btn').disabled = codePage <= 1;
  document.getElementById('codes-next-btn').disabled = r.codes.length < codePageSize;
  document.getElementById('codes-page-info').textContent = `第 ${codePage} 页`;
  document.getElementById('codes-total-info').textContent = `共 ${r.total} 条`;
}

function codesPrevPage() { if(codePage>1){ codePage--; loadCodes(); } }
function codesNextPage() { codePage++; loadCodes(); }

async function loadHealth() {
  const r = await apiFetch('/admin/health');
  if (!r) return;
  const s = r.uptime_seconds;
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  document.getElementById('health-uptime').textContent = d > 0 ? d+'天 '+h+'时 '+m+'分' : h+'时 '+m+'分 '+s%60+'秒';
  document.getElementById('health-memory').textContent = r.memory_used_mb + ' MB';
  document.getElementById('health-memory-detail').textContent = '总计 ' + r.memory_total_mb + ' MB (' + Math.round(r.memory_used_mb/r.memory_total_mb*100) + '%)';
  const dbEl = document.getElementById('health-db');
  dbEl.innerHTML = r.db_connected
    ? '<span style="color:var(--green)">● 已连接</span>'
    : '<span style="color:var(--red)">● 断开</span>';
  document.getElementById('health-db-detail').textContent = r.db_connected ? '已连接' : '未连接';
  document.getElementById('health-db-addr').textContent = r.db_connected ? 'PostgreSQL' : '—';
  document.getElementById('health-db-conns').textContent = r.db_connections + ' 个连接';
}

async function loadConfig() {
  const r = await apiFetch('/admin/config');
  if (!r) return;
  document.getElementById('cfg-token').value = r.wechat_token;
  document.getElementById('cfg-appid').value = r.wechat_appid;
  document.getElementById('cfg-appsecret-masked').value = r.wechat_appsecret_masked;
  document.getElementById('cfg-aeskey-masked').value = r.wechat_encoding_aes_key;
  document.getElementById('cfg-welcome').value = r.welcome_message;
  document.getElementById('cfg-site-name').value = r.site_name;
  document.getElementById('cfg-domain').value = r.domain;
  // Update UI with site name
  const name = r.site_name || '微信服务管理后台';
  document.getElementById('page-title').textContent = name;
  document.getElementById('brand-title').textContent = name;
  document.getElementById('login-title').textContent = name;
  document.getElementById('brand-domain').textContent = r.domain || '—';
  document.getElementById('login-subtitle').textContent = '管理员登录 · ' + (r.domain || '');
  // Update domain references in other pages
  const dom = r.domain || '—';
  const el = (id) => document.getElementById(id);
  if (el('stat-domain')) el('stat-domain').textContent = dom;
  if (el('health-domain')) el('health-domain').textContent = dom;
  if (el('guide-url')) el('guide-url').textContent = 'https://' + dom + '/wx';
}

async function saveConfig() {
  const body = {
    wechat_token:    document.getElementById('cfg-token').value,
    wechat_appid:    document.getElementById('cfg-appid').value,
    wechat_appsecret: document.getElementById('cfg-appsecret').value || undefined,
    wechat_encoding_aes_key: document.getElementById('cfg-aeskey').value || undefined,
    welcome_message: document.getElementById('cfg-welcome').value,
    site_name:       document.getElementById('cfg-site-name').value,
    domain:          document.getElementById('cfg-domain').value,
  };
  const r = await fetch(BASE+'/admin/config', {
    method: 'PUT',
    headers: { 'Content-Type':'application/json', ...authH() },
    body: JSON.stringify(body),
  });
  if (r.ok) { toast('配置已保存', 'ok'); await loadConfig(); }
  else toast('保存失败，请检查连接', 'err');
}

async function loadUsers() {
  const r = await apiFetch(`/admin/users?page=${curPage}&size=${pageSize}`);
  const tbody = document.getElementById('user-tbody');
  if (!r || !r.length) {
    tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--ink3);padding:32px">暂无数据</td></tr>';
    document.getElementById('next-btn').disabled = true;
    return;
  }
  tbody.innerHTML = r.map(u => `
    <tr>
      <td class="openid-cell">${u.openid}</td>
      <td>${u.nickname || '<span style="color:var(--ink3)">—</span>'}</td>
      <td><span class="badge ${u.subscribe ? 'badge-on':'badge-off'}">${u.subscribe ? '已关注':'已取关'}</span></td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(u.created_at)}</td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(u.updated_at)}</td>
      <td><a onclick="viewUserCodes('${u.openid}')" style="color:var(--green);font-size:12px;text-decoration:none;cursor:pointer">查看验证码</a></td>
    </tr>`).join('');
  document.getElementById('prev-btn').disabled = curPage <= 1;
  document.getElementById('next-btn').disabled = r.length < pageSize;
  document.getElementById('page-info').textContent = `第 ${curPage} 页`;
}

function prevPage() { if(curPage>1){ curPage--; loadUsers(); } }
function nextPage() { curPage++; loadUsers(); }

async function searchUsers() {
  const q = document.getElementById('user-search-input').value.trim();
  if (!q) { loadUsers(); return; }
  const r = await apiFetch(`/admin/users/search?q=${encodeURIComponent(q)}`);
  const tbody = document.getElementById('user-tbody');
  if (!r || !r.length) {
    tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--ink3);padding:32px">未找到匹配用户</td></tr>';
    document.getElementById('prev-btn').disabled = true;
    document.getElementById('next-btn').disabled = true;
    document.getElementById('page-info').textContent = '搜索结果';
    return;
  }
  tbody.innerHTML = r.map(u => `
    <tr>
      <td class="openid-cell">${u.openid}</td>
      <td>${u.nickname || '<span style="color:var(--ink3)">—</span>'}</td>
      <td><span class="badge ${u.subscribe ? 'badge-on':'badge-off'}">${u.subscribe ? '已关注':'已取关'}</span></td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(u.created_at)}</td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(u.updated_at)}</td>
      <td><a onclick="viewUserCodes('${u.openid}')" style="color:var(--green);font-size:12px;text-decoration:none;cursor:pointer">查看验证码</a></td>
    </tr>`).join('');
  document.getElementById('prev-btn').disabled = true;
  document.getElementById('next-btn').disabled = true;
  document.getElementById('page-info').textContent = `搜索: ${r.length} 条结果`;
}

function clearSearch() {
  document.getElementById('user-search-input').value = '';
  curPage = 1;
  loadUsers();
}

async function viewUserCodes(openid) {
  const r = await apiFetch(`/admin/users/${encodeURIComponent(openid)}/codes`);
  showPage('codes');
  const tbody = document.getElementById('codes-tbody');
  if (!r || !r.length) {
    tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--ink3);padding:32px">该用户暂无验证码记录</td></tr>';
    document.getElementById('codes-prev-btn').disabled = true;
    document.getElementById('codes-next-btn').disabled = true;
    document.getElementById('codes-page-info').textContent = `用户: ${openid.substring(0, 12)}…`;
    document.getElementById('codes-total-info').textContent = '共 0 条';
    return;
  }
  const now = new Date();
  tbody.innerHTML = r.map(c => {
    let badge;
    if (c.used) badge = '<span class="badge badge-on">已使用</span>';
    else if (new Date(c.expires_at) < now) badge = '<span class="badge badge-off">已过期</span>';
    else badge = '<span class="badge" style="background:var(--gold-l);color:var(--gold)">有效</span>';
    return `<tr>
      <td style="color:var(--ink3)">${c.id}</td>
      <td class="openid-cell">${c.openid}</td>
      <td class="mono" style="font-weight:600;letter-spacing:2px">${c.code}</td>
      <td>${badge}</td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(c.created_at)}</td>
      <td style="color:var(--ink3);font-size:12px">${fmtDate(c.expires_at)}</td>
    </tr>`;
  }).join('');
  document.getElementById('codes-prev-btn').disabled = true;
  document.getElementById('codes-next-btn').disabled = true;
  document.getElementById('codes-page-info').textContent = `用户: ${openid.substring(0, 12)}…`;
  document.getElementById('codes-total-info').textContent = `共 ${r.length} 条`;
}

async function changePw() {
  const p1 = document.getElementById('new-pw').value;
  const p2 = document.getElementById('new-pw2').value;
  if (!p1 || p1.length < 8) { toast('密码至少 8 位', 'err'); return; }
  if (p1 !== p2) { toast('两次密码不一致', 'err'); return; }
  const r = await fetch(BASE+'/admin/config', {
    method: 'PUT',
    headers: { 'Content-Type':'application/json', ...authH() },
    body: JSON.stringify({ new_password: p1 }),
  });
  if (r.ok) { toast('密码已更新，请重新登录', 'ok'); setTimeout(logout, 1500); }
  else toast('更新失败', 'err');
}

async function sha1hex(str) {
  const buf = await crypto.subtle.digest('SHA-1', new TextEncoder().encode(str));
  return Array.from(new Uint8Array(buf)).map(b => b.toString(16).padStart(2, '0')).join('');
}

async function testVerification() {
  const resultDiv = document.getElementById('verify-result');
  const pre = resultDiv.querySelector('pre');
  resultDiv.style.display = 'block';
  pre.textContent = '正在获取 Token 并计算签名...';

  try {
    const cfg = await apiFetch('/admin/config');
    if (!cfg || !cfg.wechat_token) {
      pre.textContent = '✗ 请先在「微信配置」中保存 Token';
      toast('请先配置 Token', 'err');
      return;
    }

    const timestamp = Math.floor(Date.now() / 1000).toString();
    const nonce = 'test_nonce_' + Math.random().toString(36).substring(7);
    const echostr = 'test_echostr_' + Date.now();
    const signature = await sha1hex([cfg.wechat_token, timestamp, nonce].sort().join(''));

    const url = `${BASE}/wx?timestamp=${timestamp}&nonce=${nonce}&echostr=${echostr}&signature=${signature}`;
    const r = await fetch(url);
    const status = r.status;
    const text = await r.text();

    pre.textContent = `状态码: ${status}\n响应: ${text}\n\n请求参数:\n  timestamp: ${timestamp}\n  nonce: ${nonce}\n  echostr: ${echostr}\n  signature: ${signature}\n  (SHA1 of sorted: token + timestamp + nonce)\n\n`;

    if (status === 200 && text === echostr) {
      pre.textContent += '✓ 验证成功！签名匹配，echostr 已正确回传。';
      toast('验证成功', 'ok');
    } else if (status === 403) {
      pre.textContent += '✗ 验证失败：签名不匹配，请检查 Token 配置。';
      toast('验证失败', 'err');
    } else {
      pre.textContent += `✗ 未预期的响应`;
      toast('验证失败', 'err');
    }
  } catch (e) {
    pre.textContent = `请求失败: ${e.message}`;
    toast('请求失败', 'err');
  }
}

async function createMenu() {
  const btn = document.getElementById('create-menu-btn');
  const result = document.getElementById('menu-result');
  btn.disabled = true;
  result.innerHTML = '<span style="color:var(--ink3)">正在创建菜单…</span>';

  try {
    const r = await fetch(BASE + '/admin/menu/create', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authH() },
    });
    const d = await r.json();
    if (d.success) {
      result.innerHTML = '<span style="color:var(--green)">✓ ' + d.message + '</span>';
      toast('菜单创建成功', 'ok');
    } else {
      result.innerHTML = '<span style="color:var(--red)">✗ ' + d.message + '</span>';
      toast('菜单创建失败', 'err');
    }
  } catch (e) {
    result.innerHTML = '<span style="color:var(--red)">请求失败: ' + e.message + '</span>';
    toast('请求失败', 'err');
  }
  btn.disabled = false;
}

// ── 工具 ──────────────────────────────────────────────────────────────────────
function authH() { return { 'Authorization': 'Bearer '+token }; }

async function apiFetch(path) {
  const r = await fetch(BASE+path, { headers: authH() });
  if (r.status === 401) { logout(); return null; }
  if (!r.ok) return null;
  return r.json();
}

function fmtDate(s) {
  if (!s) return '—';
  return new Date(s).toLocaleString('zh-CN', {year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit'});
}

function toast(msg, type) {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.className = 'show ' + type;
  setTimeout(() => el.className = '', 2500);
}
</script>
</body>
</html>
"#;
