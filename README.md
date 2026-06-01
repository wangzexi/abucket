# atree

atree is a small file-tree gateway that exposes QuarkOpen, GitHub releases, URL prefixes, S3-compatible storage, and its own live config as one path tree. It is designed for AI agents and scripts first, with S3 path-style HTTP, plain HTTP, and a minimal browser file UI.

License: MIT.

Thanks to [OpenList](https://github.com/OpenListTeam/OpenList). atree's QuarkOpen and release-mount behavior is heavily informed by OpenList's driver model, especially `drivers/quark_open`.

## Agent Quickstart

Read this file first, then inspect these source files:

- `src/main.rs`: HTTP server, S3-compatible routes, QuarkOpen client, cache, browser UI, and tests.
- `src/config.rs`: `/api/config.yaml` schema, config loading, validation, default config comments.
- `src/mounts.rs`: mount resolution for `quark_open`, `system_config`, `url_tree`, `github_releases`, and `s3`.
- `src/ui.rs`: embedded browser file UI.
- `docs/oauth-notes.md`: OpenList/QuarkOpen OAuth notes.
- `docs/auth-and-file-ui.md`: auth model and browser UI design notes.
- `docs/cache-design.md`: cache design notes.

Use `rg` before editing:

```bash
rg -n "quark_open|github_releases|system_config|ListBucket|PutObject" src docs
cargo test --quiet
```

## What The Service Does

atree serves one logical tree. S3 bucket names are path-style routing hints; object keys map to internal tree paths.

Important routes:

- `GET /?list-type=2&delimiter=/&prefix=...`: S3 ListObjectsV2 style listing.
- `GET /<path>` / `HEAD /<path>`: object read and metadata.
- `PUT /<path>` / `DELETE /<path>`: object write and delete where the mount supports it.
- `POST ?uploads`, `PUT ?partNumber=&uploadId=`, `POST ?uploadId=`, `DELETE ?uploadId=`: minimal multipart flow.
- `GET /api/config.yaml` / `PUT /api/config.yaml`: live YAML config as a mounted system file.

Authentication is atree-local. Use either:

- `Authorization: Bearer <key>`
- AWS SigV4 access key, where the access key is mapped to an atree key

AWS signatures are not fully verified; the local key mapping is the authorization boundary.

## Local Run

```bash
export ATREE_ROOT_KEY='replace-with-bootstrap-root-key'
export BIND='127.0.0.1:9000'
cargo run
```

Default SQLite config path:

```text
~/.local/share/atree/atree.sqlite
```

Useful environment variables:

- `ATREE_ROOT_KEY`: bootstrap/recovery key. Required for first config access unless explicit rules already grant access.
- `ATREE_DB`: SQLite config database path.
- `ATREE_MULTIPART_DIR`: temporary multipart upload part directory.
- `ATREE_CACHE_DIR`: object/listing cache directory.
- `BIND`: listen address, default `127.0.0.1:9000`; Docker image sets `0.0.0.0:9000`.

## Docker / GHCR

The image listens on port `9000`. Persist `/data` and point `ATREE_DB` there.

```bash
docker run --rm \
  -p 9000:9000 \
  -e ATREE_ROOT_KEY='replace-with-bootstrap-root-key' \
  -e ATREE_DB='/data/atree.sqlite' \
  -v atree-data:/data \
  ghcr.io/<owner>/atree:latest
```

For this repository's GitHub Actions image, tags are:

```text
ghcr.io/<owner>/atree:latest
ghcr.io/<owner>/atree:<git-sha>
```

Kubernetes shape:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: atree
spec:
  selector:
    matchLabels:
      app: atree
  template:
    metadata:
      labels:
        app: atree
    spec:
      containers:
        - name: atree
          image: ghcr.io/<owner>/atree:latest
          ports:
            - containerPort: 9000
          env:
            - name: ATREE_ROOT_KEY
              valueFrom:
                secretKeyRef:
                  name: atree
                  key: root-key
            - name: ATREE_DB
              value: /data/atree.sqlite
          volumeMounts:
            - name: data
              mountPath: /data
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: atree-data
```

## Configure

Fetch config:

```bash
curl -H 'Authorization: Bearer <root-key>' \
  'http://127.0.0.1:9000/api/config.yaml' > config.yaml
```

Update config:

```bash
curl -X PUT \
  -H 'Authorization: Bearer <root-key>' \
  --data-binary @config.yaml \
  'http://127.0.0.1:9000/api/config.yaml'
```

`plain_key` is accepted only on write. The service stores `key_hash` and `key_hint`, and `GET /api/config.yaml` does not return plaintext keys.

Minimal config:

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

QuarkOpen mount:

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
```

OpenList upstream notes:

- OpenList `drivers/quark_open/meta.go` defaults `api_url_address` to `https://api.oplist.org/quarkyun/renewapi`.
- OpenList refresh requests use `refresh_ui`, `server_use=true`, and `driver_txt=quarkyun_oa`.
- atree can use that endpoint when `app_id/sign_key` are already configured.
- If atree must learn `app_id/sign_key`, use a complete refresh endpoint such as `https://oauth.fnnas.com/api/v1/oauth/refreshToken`.

GitHub release mount:

```yaml
mounts:
  - mount_path: /client
    type: github_releases
    root_path: OpenListTeam/OpenList
    options:
      show_source_code: true
      asset_allow:
        - openlist-*.tar.gz
auth:
  rules:
    - principal: anonymous
      actions: [ListBucket, HeadObject, GetObject]
      resources: [/client, /client/*]
```

URL prefix mount:

```yaml
mounts:
  - mount_path: /downloads
    type: url_tree
    root_path: https://github.com/OpenListTeam/OpenList/releases/latest/download
    options:
      proxy: http://127.0.0.1:1080
```

S3-compatible mount:

```yaml
mounts:
  - mount_path: /files
    type: s3
    root_path: /
    options:
      endpoint: http://minio.minio.svc.cluster.local:9000
      bucket: files
      region: us-east-1
      access_key: '<private access key>'
      secret_key: '<private secret key>'
      path_style: true
```

## Verify

List:

```bash
curl -H 'Authorization: Bearer <key>' \
  'http://127.0.0.1:9000/?list-type=2&delimiter=/&prefix=quark/'
```

Upload and read:

```bash
echo hello > /tmp/atree.txt
curl -H 'Authorization: Bearer <key>' \
  -T /tmp/atree.txt \
  'http://127.0.0.1:9000/quark/examples/atree.txt'
curl -H 'Authorization: Bearer <key>' \
  'http://127.0.0.1:9000/quark/examples/atree.txt'
```

AWS CLI anonymous listing when config allows anonymous access:

```bash
aws --endpoint-url http://127.0.0.1:9000 s3 ls s3://atree --no-sign-request
```

restic:

```bash
export RESTIC_PASSWORD='replace-with-restic-password'
export AWS_ACCESS_KEY_ID='<atree-key>'
export AWS_SECRET_ACCESS_KEY=dummy

restic -r 's3:http://127.0.0.1:9000/quark/restic-repo' \
  -o s3.bucket-lookup=path \
  init
```

## Safety Rules For Agents

- Never commit live `refresh_token`, `access_token`, `app_id`, `sign_key`, S3 keys, root keys, SQLite databases, or config snapshots.
- Keep secrets in runtime config, Kubernetes Secrets, or local ignored files.
- Run `cargo test --quiet` after code changes.
- If changing config schema, update `src/config.rs`, README examples, and tests together.
- If changing mount resolution, inspect `src/mounts.rs` and related tests in `src/main.rs`.
- If changing Docker behavior, update Docker run examples and `.github/workflows/ci.yml` expectations.
