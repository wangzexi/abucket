# atree

AI 友好的文件树网关。协议 MIT。感谢 [OpenList](https://github.com/OpenListTeam/OpenList)，QuarkOpen 相关逻辑参考 `drivers/quark_open`。

## 先看代码

- `src/main.rs`：主逻辑、路由、QuarkOpen、测试。
- `src/config.rs`：配置 schema、默认配置、校验。
- `src/mounts.rs`：mount 解析。
- `src/ui.rs`：内嵌文件浏览器。
- `docs/oauth-notes.md`：OpenList/QuarkOpen 记录。

```bash
rg -n "quark_open|github_releases|system_config|ListBucket|PutObject" src docs
cargo test --quiet
```

## 本地运行

```bash
export ATREE_ROOT_KEY='replace-with-root-key'
export BIND='127.0.0.1:9000'
cargo run
```

环境变量看代码：`ATREE_ROOT_KEY`、`ATREE_DB`、`ATREE_MULTIPART_DIR`、`ATREE_CACHE_DIR`、`BIND`。

## Docker

```bash
docker run --rm \
  -p 9000:9000 \
  -e ATREE_ROOT_KEY='replace-with-root-key' \
  -e ATREE_DB='/data/atree.sqlite' \
  -v atree-data:/data \
  ghcr.io/<owner>/atree:latest
```

镜像：`ghcr.io/<owner>/atree:latest` 或 `ghcr.io/<owner>/atree:<git-sha>`。

K8s：持久化 `/data`，`ATREE_ROOT_KEY` 用 Secret，`ATREE_DB=/data/atree.sqlite`。

## 配置入口

```bash
curl -H 'Authorization: Bearer <root-key>' \
  'http://127.0.0.1:9000/api/config.yaml' > config.yaml

curl -X PUT \
  -H 'Authorization: Bearer <root-key>' \
  --data-binary @config.yaml \
  'http://127.0.0.1:9000/api/config.yaml'
```

最小配置：

```yaml
s3_bucket: atree
mounts:
  - mount_path: /api/config.yaml
    type: system_config
auth:
  keys:
    - name: admin
      plain_key: replace-with-admin-key
  rules:
    - principal: key:admin
      actions: [ListBucket, HeadObject, GetObject, PutObject, DeleteObject]
      resources: [/*]
```

## Mount 示例

```yaml
mounts:
  - mount_path: /quark
    type: quark_open
    root_path: /
    options:
      refresh_url: https://api.oplist.org/quarkyun/renewapi
      refresh_token: '<private refresh token>'
      access_token: '<optional current access token>'
      app_id: '<private app id>'
      sign_key: '<private sign key>'

  - mount_path: /client
    type: github_releases
    root_path: OpenListTeam/OpenList
    options:
      show_source_code: true
      asset_allow: [openlist-*.tar.gz]

  - mount_path: /downloads
    type: url_tree
    root_path: https://github.com/OpenListTeam/OpenList/releases/latest/download

  - mount_path: /files
    type: s3
    root_path: /
    options:
      endpoint: http://minio.minio.svc.cluster.local:9000
      bucket: files
      access_key: '<private access key>'
      secret_key: '<private secret key>'
```

OpenList QuarkOpen 默认刷新接口是 `https://api.oplist.org/quarkyun/renewapi`。如果需要让 atree 获取 `app_id/sign_key`，看 `docs/oauth-notes.md`。

## 快速验证

```bash
curl -H 'Authorization: Bearer <key>' \
  'http://127.0.0.1:9000/?list-type=2&delimiter=/&prefix=quark/'

echo hello > /tmp/atree.txt
curl -H 'Authorization: Bearer <key>' -T /tmp/atree.txt \
  'http://127.0.0.1:9000/quark/examples/atree.txt'
```

## Agent 规则

- 不提交真实 token、key、SQLite、配置快照。
- 改代码跑 `cargo test --quiet`。
- 不确定的行为直接看 `src/main.rs` 测试。
