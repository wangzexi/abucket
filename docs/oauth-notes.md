# QuarkOpen OAuth Notes

atree only supports the QuarkOpen OAuth driver for Quark mounts. The older web-cookie driver was removed so there is one Quark auth model to understand and operate.

## OpenList Findings

OpenList has a `quark_open` driver:

- reference path: OpenList's `drivers/quark_open`
- API base: `https://open-api-drive.quark.cn`
- main APIs:
  - `/open/v1/user/info`
  - `/open/v1/file/list`
  - `/open/v1/file/get_download_url`
  - `/open/v1/file/upload_pre`
  - `/open/v1/file/get_upload_urls`
  - `/open/v1/file/upload_finish`

It uses:

- `access_token`
- `refresh_token`
- `app_id`
- `sign_key`
- `x-pan-tm`
- `x-pan-token`
- `x-pan-client-id`

`x-pan-token` is generated as:

```text
sha256(method + "&" + pathname + "&" + timestamp_ms + "&" + sign_key)
```

Upload also requires proof fields:

- `proof_version`
- `proof_seed1`
- `proof_seed2`
- `proof_code1`
- `proof_code2`

OpenList computes proof ranges from MD5-derived offsets and base64-encodes the selected bytes.

## Refresh Path

OpenList's `quark_open` driver sets the default online token refresh path in `drivers/quark_open/meta.go`:

```text
https://api.oplist.org/quarkyun/renewapi
```

The OpenList refresh request is implemented in `drivers/quark_open/util.go` with query parameters `refresh_ui`, `server_use=true`, and `driver_txt=quarkyun_oa`. That response shape is enough for OpenList's stored driver config when `app_id` and `sign_key` are already known. atree stores Quark OAuth state directly in mount options, so it can use the OpenList renew API for access/refresh token rotation, but a first-time config without `app_id/sign_key` should use a refresh endpoint that returns the signing fields too. The underlying FnOS Quark OAuth refresh endpoint returns them under `data.tokenInfo.appId` and `data.tokenInfo.signKey`:

```yaml
source:
  refresh_url: https://oauth.fnnas.com/api/v1/oauth/refreshToken
```

OpenList's public renew API does not need to expose `sign_key` in the normal refresh response because the driver already has its application fields. atree stores the refreshed `access_token`, `refresh_token`, `app_id`, and `sign_key` in the corresponding `quark_open` mount's `options` inside `/api/config.yaml`.

This has been tested with real Quark OAuth credentials: atree can list the root directory and complete a small-object PUT, GET, byte-for-byte readback, and DELETE loop through the `quark_open` mount.

## Operating Model

1. Use `type: quark_open` for Quark mounts.
2. Store OAuth state in that mount's `options`: `refresh_token`, optional current `access_token`, `app_id`, `sign_key`, and `refresh_url`.
3. Refresh rotated tokens back into the same mount options in `/api/config.yaml`.
4. Use `https://api.oplist.org/quarkyun/renewapi` when the mount already has `app_id/sign_key`; use `https://oauth.fnnas.com/api/v1/oauth/refreshToken` or another complete refresh endpoint when atree must also learn those signing fields.

## Near-Term Improvements

- Move large uploads away from whole-object-in-memory buffering.
- Add a fully local `quark_open` refresh implementation if Quark/FnOS publishes a stable documented protocol.
