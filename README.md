# quark-s3-demo

一个 Rust 网关：把夸克网盘目录包装成 S3 path-style HTTP 服务，当前主要目标是给 restic 备份使用。

它参考了 `refs/alist/drivers/quark_uc` 里的 AList 夸克驱动，目前实现：

- `GET /`：列 bucket
- `GET /quark?list-type=2&delimiter=/&prefix=...`：列对象和目录
- `GET /quark/<key>`：下载对象
- `HEAD /quark/<key>`：对象元信息
- `PUT /quark/<key>`：上传对象，必要时自动创建父目录
- `DELETE /quark/<key>`：删除对象
- `GET /quark/<key>` + `Range`：范围读取，供 restic 读取 pack 片段

这不是完整 S3 实现，暂时没有校验 AWS Signature，也没有实现 S3 multipart upload API。它优先覆盖 restic 使用到的 S3 语义。

## 运行

```bash
cargo run
source ./quark.env
cargo run
```

`quark.env` 放在项目根目录，包含 `QUARK_COOKIE` 等本地敏感配置，已被 `.gitignore` 忽略。

## 简单测试

不做签名校验，所以可以用 AWS CLI 的匿名模式：

```bash
aws --endpoint-url http://127.0.0.1:9000 s3 ls s3://quark --no-sign-request
echo hello > /tmp/quark-s3-demo.txt
aws --endpoint-url http://127.0.0.1:9000 s3 cp /tmp/quark-s3-demo.txt s3://quark/demo/quark-s3-demo.txt --no-sign-request
aws --endpoint-url http://127.0.0.1:9000 s3 cp s3://quark/demo/quark-s3-demo.txt - --no-sign-request
aws --endpoint-url http://127.0.0.1:9000 s3 rm s3://quark/demo/quark-s3-demo.txt --no-sign-request
```

## restic 使用

```bash
cargo run
source ./quark.env
cargo run
```

另一个终端：

```bash
export RESTIC_PASSWORD='你的 restic 仓库密码'
export AWS_ACCESS_KEY_ID=dummy
export AWS_SECRET_ACCESS_KEY=dummy

restic -r 's3:http://127.0.0.1:9000/quark/restic-repo' \
  -o s3.bucket-lookup=path \
  init

restic -r 's3:http://127.0.0.1:9000/quark/restic-repo' \
  -o s3.bucket-lookup=path \
  backup ~/Documents
```

如果要放到夸克的某个目录里，可以直接把目录写进 repo path，例如：

```bash
restic -r 's3:http://127.0.0.1:9000/quark/我的备份/restic-repo' \
  -o s3.bucket-lookup=path \
  snapshots
```

这里的 AWS key 只是为了让 MinIO SDK 走签名请求；网关目前不校验签名。

也可以用 `curl`：

```bash
curl 'http://127.0.0.1:9000/quark?list-type=2&delimiter=/'
curl -T /tmp/quark-s3-demo.txt 'http://127.0.0.1:9000/quark/demo/quark-s3-demo.txt'
curl 'http://127.0.0.1:9000/quark/demo/quark-s3-demo.txt'
```

## 配置项

- `QUARK_COOKIE`：夸克网页登录态 Cookie。
- `QUARK_ROOT_FID`：对外暴露的夸克目录 fid，默认 `0`。
- `S3_BUCKET`：本地 S3 bucket 名，默认 `quark`。
- `BIND`：监听地址，默认 `127.0.0.1:9000`。
- `MAX_UPLOAD_BYTES`：单个 PUT 最大请求体，默认 `134217728`，也就是 128 MiB。

## 已知限制

- 上传会把单个对象读进内存后再走夸克上传流程。restic 默认 pack 约 16 MiB，当前够用；如调大 `--pack-size`，注意 `MAX_UPLOAD_BYTES` 和内存。
- S3 XML 返回只覆盖常见字段，兼容性主要面向 restic、`aws s3`/`curl` 的基础操作。
- 下载会代理夸克下载链接，而不是返回 302。
- Cookie 自动刷新只保存在进程内，没有写回磁盘。

## 已验证

- `restic init` 成功。
- 备份 8 MiB 测试目录成功，`restic check` 无错误，`restore latest` 成功。
- 备份 16 MiB 随机文件成功，总耗时约 18.8 秒；恢复耗时约 5.8 秒。

## 后续 OAuth/Open API 方向

当前 demo 走网页登录 Cookie。关于后续迁移到 OpenList `quark_open` / OAuth-style token 的记录见 `docs/oauth-notes.md`。

## 本地缓存方向

关于 Rust vs Bun、SQLite、write-through 写入、read-through 读取缓存的设计决策见 `docs/cache-design.md`。

## 鉴权和文件界面

关于 API key、公开路径、浏览器 `localStorage` 登录和 Quark-backed file browser 的设计见 `docs/auth-and-file-ui.md`。
