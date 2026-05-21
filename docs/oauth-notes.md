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

## Important Risk

OpenList's `quark_open` local refresh-token flow is not implemented. Its default token refresh path calls:

```text
https://api.oplist.org/quarkyun/renewapi
```

That means the refresh token is sent to a third-party service unless a local refresh implementation is added. Do not enable that by default in this service.

## Migration Plan

1. Keep the current cookie backend as `quark_uc`.
2. Add a backend enum/config, for example:
   - `QUARK_BACKEND=cookie`
   - `QUARK_BACKEND=open`
3. Add `quark_open` config:
   - `QUARK_ACCESS_TOKEN`
   - `QUARK_REFRESH_TOKEN`
   - `QUARK_APP_ID`
   - `QUARK_SIGN_KEY`
   - optional `QUARK_OPEN_REFRESH_URL`
4. Implement `quark_open` list/download first.
5. Implement upload after list/download works, including proof generation.
6. Keep third-party refresh disabled unless explicitly configured.
7. If local refresh protocol is discovered, implement local refresh and persist rotated tokens to a private config file.

## Near-Term Improvements For Cookie Mode

- Persist refreshed `__puus` / `__pus` back to the relevant `quark_cookie` mount config, or to another private state file if we decide not to expose refreshed cookies in config output.
- Move large uploads away from whole-object-in-memory buffering.
- Add a local `quark_open` refresh implementation before enabling OAuth-style credentials by default.
