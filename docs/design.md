# atree 设计思路

atree 是一个个人自托管文件树网关：多个后端挂成同一棵路径树，浏览器访问是文件树界面，API 访问是 S3 path-style 协议。

## Model

```text
mounts -> one path tree -> browser file tree
                       -> S3 path-style API
                       -> /api/config.yaml
```

配置本身也是树上的一个文件。`system_config` mount 默认暴露 `/api/config.yaml`，AI 或脚本可以直接读取、修改这个文件来管理服务。

## Config

顶层配置只保留几类对象：

- `bucket`: S3 path-style bucket name。
- `mounts`: 后端挂载表，后面的 mount 优先级更高。
- `users`: 额外用户；`root` 来自 `ATREE_ROOT_KEY`，`anonymous` 是未带 key 的访问。
- `rules`: 按用户、路径、动作授权；没有匹配规则就拒绝，`root` 除外。
- `cache`: 本地缓存开关和 TTL。

完整字段说明由 `src/config.rs` 的 `config_yaml_comments()` 生成，校验规则看 `validate_config()`。

## Mounts

当前 driver：

- `system_config`: 把 live config 暴露成树上的一个文件。
- `quark_open`: 通过 QuarkOpen OAuth 读写夸克网盘。
- `s3`: 代理 S3 兼容对象存储，使用 path-style 请求。
- `url_tree`: 只读 URL 前缀映射。
- `github_releases`: 只读 GitHub Release assets 映射。

每个 driver 的配置入口放在 `src/drivers/*.rs`，运行时解析由 `src/mounts.rs` 串起来。

## Auth

权限模型只有三件事：

- user: `root`、`anonymous` 或 `users[].name`。
- paths: 例如 `/public`、`/public/*`、`/*`。
- actions: `ListBucket`、`HeadObject`、`GetObject`、`PutObject`、`DeleteObject` 或 `*`。

`rules` 只授权，不创建存储路径。可写路径必须同时命中可写 mount。

## Cache

缓存用于降低远端列表和对象读取成本。写入仍以远端成功为准，本地缓存不是最终存储。
