use axum::http::{HeaderMap, header};

pub(crate) fn wants_html(headers: &HeaderMap) -> bool {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !accept
        .split(',')
        .any(|part| part.trim().starts_with("text/html"))
    {
        return false;
    }
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    ![
        "curl",
        "wget",
        "aws-cli",
        "boto3",
        "restic",
        "rclone",
        "go-http-client",
        "python-requests",
    ]
    .iter()
    .any(|tool| ua.contains(tool))
}

pub(crate) fn file_browser_html(bucket: &str, virtual_path: &str) -> String {
    let bucket_json = serde_json::to_string(bucket).unwrap_or_else(|_| "\"quark\"".to_string());
    let path_json = serde_json::to_string(virtual_path).unwrap_or_else(|_| "\"/\"".to_string());
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>atree</title>
  <style>
    :root {{ color-scheme: light dark; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    body {{ margin: 0; background: Canvas; color: CanvasText; }}
    header {{ display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 20px; border-bottom: 1px solid color-mix(in srgb, CanvasText 14%, transparent); }}
    main {{ max-width: 1040px; margin: 0 auto; padding: 18px 20px 40px; }}
    button, input {{ font: inherit; }}
    button {{ border: 1px solid color-mix(in srgb, CanvasText 18%, transparent); background: ButtonFace; color: ButtonText; border-radius: 6px; padding: 7px 10px; cursor: pointer; }}
    input {{ min-width: 220px; border: 1px solid color-mix(in srgb, CanvasText 18%, transparent); border-radius: 6px; padding: 8px 10px; background: Field; color: FieldText; }}
    .bar {{ display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin-bottom: 14px; }}
    .auth {{ display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }}
    .crumbs {{ display: flex; gap: 6px; flex-wrap: wrap; align-items: center; font-size: 14px; }}
    .crumbs a {{ color: LinkText; text-decoration: none; }}
    table {{ width: 100%; border-collapse: collapse; }}
    th, td {{ padding: 10px 8px; border-bottom: 1px solid color-mix(in srgb, CanvasText 12%, transparent); text-align: left; }}
    th.size, td.size {{ width: 120px; text-align: right; }}
    th.time, td.time {{ width: 210px; }}
    a {{ color: LinkText; }}
    .muted {{ color: color-mix(in srgb, CanvasText 62%, transparent); }}
    .error {{ color: #b42318; }}
  </style>
</head>
<body>
  <header>
    <strong>atree</strong>
    <button id="copyHelp" type="button">复制 API help curl</button>
  </header>
  <main>
    <div class="bar">
      <nav id="crumbs" class="crumbs"></nav>
      <div class="auth">
        <span id="authState" class="muted"></span>
        <input id="keyInput" type="password" autocomplete="current-password" placeholder="访问 key">
        <button id="saveKey" type="button">保存</button>
        <button id="clearKey" type="button">清除</button>
      </div>
    </div>
    <p id="message" class="muted">加载中...</p>
    <table>
      <thead><tr><th>名称</th><th class="size">大小</th><th class="time">更新时间</th></tr></thead>
      <tbody id="rows"></tbody>
    </table>
  </main>
  <script>
    const BUCKET = {bucket_json};
    const INITIAL_PATH = {path_json};
    const keyName = 'atree_key';
    const keyInput = document.getElementById('keyInput');
    const authState = document.getElementById('authState');
    const message = document.getElementById('message');
    const rows = document.getElementById('rows');
    const crumbs = document.getElementById('crumbs');

    function currentKey() {{ return localStorage.getItem(keyName) || ''; }}
    function setAuthState() {{ authState.textContent = currentKey() ? '已保存 key' : '匿名访问'; }}
    function s3Path() {{
      const path = location.pathname === '/' ? '/' + BUCKET + '/' : location.pathname;
      return path.endsWith('/') ? path : path + '/';
    }}
    function keyPrefixFromPath() {{
      const parts = s3Path().split('/').filter(Boolean);
      if (parts[0] === BUCKET) parts.shift();
      return parts.length ? parts.join('/') + '/' : '';
    }}
    function listUrl() {{
      const u = new URL(s3Path(), location.origin);
      u.searchParams.set('list-type', '2');
      u.searchParams.set('delimiter', '/');
      const prefix = keyPrefixFromPath();
      if (prefix) u.searchParams.set('prefix', prefix);
      return u;
    }}
    function headers(accept = 'application/xml') {{
      const h = {{ 'Accept': accept }};
      const key = currentKey();
      if (key) h.Authorization = 'Bearer ' + key;
      return h;
    }}
    function fmtBytes(n) {{
      if (!n) return '';
      const units = ['B','KiB','MiB','GiB','TiB'];
      let v = Number(n), i = 0;
      while (v >= 1024 && i < units.length - 1) {{ v /= 1024; i++; }}
      return (i ? v.toFixed(1) : v.toFixed(0)) + ' ' + units[i];
    }}
    function renderCrumbs() {{
      const parts = keyPrefixFromPath().split('/').filter(Boolean);
      const links = [`<a href="/${{BUCKET}}/">/${{BUCKET}}</a>`];
      let acc = '';
      for (const part of parts) {{
        acc += encodeURIComponent(part) + '/';
        links.push(`<span>/</span><a href="/${{BUCKET}}/${{acc}}">${{part}}</a>`);
      }}
      crumbs.innerHTML = links.join('');
    }}
    async function load() {{
      setAuthState();
      renderCrumbs();
      rows.innerHTML = '';
      message.textContent = '加载中...';
      const res = await fetch(listUrl(), {{ headers: headers() }});
      if (res.status === 403 || res.status === 401) {{
        message.innerHTML = '<span class="error">需要访问 key。</span>';
        return;
      }}
      if (!res.ok) {{
        message.innerHTML = '<span class="error">列表失败：' + res.status + '</span>';
        return;
      }}
      const doc = new DOMParser().parseFromString(await res.text(), 'application/xml');
      const prefix = doc.querySelector('Prefix')?.textContent || keyPrefixFromPath();
      const items = [];
      doc.querySelectorAll('CommonPrefixes > Prefix').forEach(el => {{
        const full = el.textContent || '';
        const name = full.slice(prefix.length).replace(/\/$/, '');
        if (name) items.push({{ type: 'dir', name, href: '/' + BUCKET + '/' + full }});
      }});
      doc.querySelectorAll('Contents').forEach(el => {{
        const full = el.querySelector('Key')?.textContent || '';
        const name = full.slice(prefix.length);
        if (!name || name.includes('/')) return;
        items.push({{
          type: 'file',
          name,
          href: '/' + BUCKET + '/' + full,
          size: el.querySelector('Size')?.textContent || '',
          time: el.querySelector('LastModified')?.textContent || ''
        }});
      }});
      message.textContent = items.length ? '' : '空目录';
      rows.innerHTML = items.map(item => `
        <tr>
          <td>${{item.type === 'dir' ? '[dir]' : '[file]'}} <a href="${{item.href}}">${{item.name}}</a></td>
          <td class="size">${{item.type === 'file' ? fmtBytes(item.size) : ''}}</td>
          <td class="time muted">${{item.time || ''}}</td>
        </tr>
      `).join('');
    }}
    document.getElementById('saveKey').onclick = () => {{ localStorage.setItem(keyName, keyInput.value); keyInput.value = ''; load(); }};
    document.getElementById('clearKey').onclick = () => {{ localStorage.removeItem(keyName); load(); }};
    document.getElementById('copyHelp').onclick = async () => {{
      const cmd = `curl -H 'Authorization: Bearer <super-admin-key>' '${{location.origin}}/api/help'`;
      await navigator.clipboard.writeText(cmd);
      message.textContent = '已复制：' + cmd;
    }};
    load().catch(err => {{ message.innerHTML = '<span class="error">' + err.message + '</span>'; }});
  </script>
</body>
</html>"#
    )
}
