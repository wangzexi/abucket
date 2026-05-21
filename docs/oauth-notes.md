# Quark OAuth Migration Notes

The current service uses the Quark web-cookie flow, following OpenList's `quark_uc` driver.

## Current Mode

- `quark_cookie` mount `options.cookie` is captured from a logged-in `https://pan.quark.cn` browser session.
- The service calls Quark web APIs such as:
  - `https://drive.quark.cn/1/clouddrive/file/sort`
  - `https://drive.quark.cn/1/clouddrive/file/download`
  - `https://drive.quark.cn/1/clouddrive/file/upload/pre`
- This mode has been validated for list, read, write, and delete against:
  - `Backups/example/`

## Limitation

Cookies expire. The service updates `__puus` / `__pus` in memory when Quark returns new `Set-Cookie` headers, but it does not persist refreshed cookies back to disk yet.

For unattended long-running use, web cookies are weaker than OAuth-style credentials.

## OpenList QuarkOpen Findings

OpenList also has a `quark_open` driver:

- Reference path: `OpenList drivers/quark_open`
- API base: `https://open-api-drive.quark.cn`
- Main APIs:
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

OpenList's `quark_open` local refresh-token flow is not implemented. Its default token refresh path calls:

```text
https://api.oplist.org/quarkyun/renewapi
```

That means the refresh token is sent to a third-party service unless a local refresh implementation is added.

For the self-hosted APIPages flow used by `oauth.example.com`, `/quarkyun/renewapi` returns only `access_token` and `refresh_token`. The underlying FnOS Quark OAuth refresh endpoint returns the app signing fields under `data.tokenInfo.appId` and `data.tokenInfo.signKey`, so atree uses that endpoint directly when the private OAuth YAML sets:

```yaml
source:
  refresh_url: https://oauth.fnnas.com/api/v1/oauth/refreshToken
```

The token page does not print `sign_key` because OpenList APIPages normally treats application keys as server-side credentials.

This has been tested with real Quark OAuth credentials: atree can list the root directory and complete a small-object PUT, GET, byte-for-byte readback, and DELETE loop through the `quark_open` mount.

## Migration Plan

1. Keep the current cookie backend as `quark_cookie`.
2. Use `type: quark_open` for OAuth-backed mounts.
3. Store OAuth secrets in a private ignored YAML, then reference it from mount `options.oauth_file`.
4. Refresh rotated access/refresh tokens back into the private OAuth YAML.
5. Keep `source.refresh_url` pointed at `https://oauth.fnnas.com/api/v1/oauth/refreshToken` for FnOS-backed Quark OAuth tokens; the OpenList APIPages renew endpoint is not enough for atree because it omits `sign_key`.

## Near-Term Improvements For Cookie Mode

- Persist refreshed `__puus` / `__pus` back to the relevant `quark_cookie` mount config, or to another private state file if we decide not to expose refreshed cookies in config output.
- Move large uploads away from whole-object-in-memory buffering.
- Add a fully local `quark_open` refresh implementation if Quark/FnOS publishes a stable documented protocol.
