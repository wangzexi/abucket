# abucket

适合个人自托管且 AI 友好的文件树 S3 网关。

- 多个后端挂成一棵路径树。
- 浏览器访问是文件树界面。
- API 访问是 S3 path-style 协议。
- 配置也是树上的文件：`/api/config.yaml`。
- 权限模型只有用户、路径和动作。

```mermaid
flowchart LR
  Q["QuarkOpen"]
  G["GitHub Release"]
  U["URL prefix"]
  S["S3 storage"]
  C["/api/config.yaml"]

  Q --> T["abucket path tree"]
  G --> T
  U --> T
  S --> T
  C --> T

  T --> B["browser: file tree"]
  T --> A["API: S3 path-style"]
  A --> AI["AI / scripts"]
  AI --> C
```

## Docker

```bash
docker run --rm \
  -p 9000:9000 \
  -e ABUCKET_ROOT_KEY='RofCYxijYM' \
  -e ABUCKET_DB='/data/abucket.sqlite' \
  -v abucket-data:/data \
  ghcr.io/wangzexi/abucket:latest
```

## 配置

```bash
curl -H 'Authorization: Bearer <root-key>' \
  'http://127.0.0.1:9000/api/config.yaml' > config.yaml

curl -X PUT \
  -H 'Authorization: Bearer <root-key>' \
  --data-binary @config.yaml \
  'http://127.0.0.1:9000/api/config.yaml'
```

```yaml
bucket: abucket
mounts:
  - type: system_config
    path: /api/config.yaml
  - type: s3
    path: /public
    options:
      endpoint: https://s3.example.com
      bucket: public
      access_key: 20i9WXV8Tx
      secret_key: lrV7zeNdkL
users:
  - name: public
    key: Ok4t2IR4Bp
rules:
  - user: root
    paths: [/, /*]
    actions: [ListBucket, HeadObject, GetObject, PutObject, DeleteObject]
  - user: anonymous
    paths: [/]
    actions: [ListBucket]
  - user: public
    paths: [/public, /public/*]
    actions: [ListBucket, HeadObject, GetObject, PutObject]
cache:
  enabled: true
  ttl_seconds: 600
```

内置用户有两个：`root` 来自 `ABUCKET_ROOT_KEY`（兼容 `ATREE_ROOT_KEY`），拥有管理入口；`anonymous` 是未带 key 的访问。它们都不用写进 `users`。`rules` 只授权；可写路径还需要实际可写 mount。

完整配置注释由代码生成：看 `src/config.rs` 的 `config_yaml_comments()` 和 `validate_config()`。Mount 配置看 `src/mounts/*.rs`。

## 致谢

- [OpenList](https://github.com/OpenListTeam/OpenList)

## 协议

MIT
