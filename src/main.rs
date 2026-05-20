use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use base64::{Engine as _, engine::general_purpose};
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tokio::{net::TcpListener, sync::Mutex, time::sleep};
use tracing::{info, warn};

const QUARK_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/2.5.20 Chrome/100.0.4896.160 Electron/18.3.5.4-b478491100 Safari/537.36 Channel/pckk_other_ch";
const REFERER: &str = "https://pan.quark.cn";
const API: &str = "https://drive.quark.cn/1/clouddrive";
const PR: &str = "ucpro";

#[derive(Clone)]
struct AppState {
    quark: QuarkClient,
    bucket: String,
}

#[derive(Clone)]
struct QuarkClient {
    http: Client,
    cookie: Arc<Mutex<String>>,
    root_fid: String,
}

#[derive(Debug, Deserialize)]
struct ApiStatus {
    #[serde(default)]
    status: i64,
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct QuarkFile {
    fid: String,
    file_name: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    file: bool,
    #[serde(default)]
    created_at: i64,
    #[serde(default)]
    updated_at: i64,
}

#[derive(Debug, Deserialize)]
struct SortResp {
    data: SortData,
    metadata: SortMeta,
}

#[derive(Debug, Deserialize)]
struct SortData {
    list: Vec<QuarkFile>,
}

#[derive(Debug, Deserialize)]
struct SortMeta {
    #[serde(rename = "_total")]
    total: usize,
}

#[derive(Debug, Deserialize)]
struct DownResp {
    data: Vec<DownItem>,
}

#[derive(Debug, Deserialize)]
struct DownItem {
    download_url: String,
}

#[derive(Debug, Deserialize)]
struct UpPreResp {
    data: UpPreData,
    metadata: UpPreMeta,
}

#[derive(Debug, Clone, Deserialize)]
struct UpPreData {
    task_id: String,
    #[serde(default)]
    finish: bool,
    upload_id: String,
    obj_key: String,
    upload_url: String,
    bucket: String,
    auth_info: String,
    callback: Value,
}

#[derive(Debug, Deserialize)]
struct UpPreMeta {
    part_size: usize,
}

#[derive(Debug, Deserialize)]
struct HashResp {
    data: HashData,
}

#[derive(Debug, Deserialize)]
struct HashData {
    #[serde(default)]
    finish: bool,
}

#[derive(Debug, Deserialize)]
struct UpAuthResp {
    data: UpAuthData,
}

#[derive(Debug, Deserialize)]
struct UpAuthData {
    auth_key: String,
}

impl QuarkClient {
    fn new(cookie: String, root_fid: String) -> Result<Self> {
        let http = Client::builder()
            .user_agent(QUARK_UA)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;
        Ok(Self {
            http,
            cookie: Arc::new(Mutex::new(cookie)),
            root_fid,
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        pathname: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<T> {
        let url = format!("{API}{pathname}");
        let cookie = self.cookie.lock().await.clone();
        let mut req = self
            .http
            .request(method, url)
            .header(header::COOKIE, cookie)
            .header(header::ACCEPT, "application/json, text/plain, */*")
            .header(header::REFERER, REFERER)
            .query(&[("pr", PR), ("fr", "pc")])
            .query(query);
        if let Some(body) = body {
            req = req.json(&body);
        }

        let res = req.send().await?;
        self.update_cookie(res.headers()).await;
        let status = res.status();
        let bytes = res.bytes().await?;
        if !status.is_success() {
            bail!(
                "quark api http {}: {}",
                status,
                String::from_utf8_lossy(&bytes)
            );
        }

        let api: ApiStatus = serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "invalid quark response: {}",
                String::from_utf8_lossy(&bytes)
            )
        })?;
        if api.status >= 400 || api.code != 0 {
            bail!(
                "quark api error status={} code={}: {}",
                api.status,
                api.code,
                api.message
            );
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn update_cookie(&self, headers: &HeaderMap) {
        let mut cookie = self.cookie.lock().await;
        for value in headers.get_all(header::SET_COOKIE) {
            let Ok(s) = value.to_str() else { continue };
            for name in ["__puus", "__pus"] {
                if let Some(v) = parse_set_cookie_value(s, name) {
                    *cookie = set_cookie_value(&cookie, name, &v);
                }
            }
        }
    }

    async fn list_files(&self, parent_fid: &str) -> Result<Vec<QuarkFile>> {
        let mut files = Vec::new();
        let mut page = 1usize;
        let size = 100usize;
        loop {
            let resp: SortResp = self
                .request(
                    Method::GET,
                    "/file/sort",
                    &[
                        ("pdir_fid", parent_fid.to_string()),
                        ("_size", size.to_string()),
                        ("_page", page.to_string()),
                        ("_fetch_total", "1".into()),
                        ("fetch_all_file", "1".into()),
                        ("fetch_risk_file_name", "1".into()),
                        ("_sort", "file_type:asc,file_name:asc".into()),
                    ],
                    None,
                )
                .await?;
            files.extend(resp.data.list);
            if page * size >= resp.metadata.total {
                break;
            }
            page += 1;
        }
        Ok(files)
    }

    async fn mkdir(&self, parent_fid: &str, name: &str) -> Result<()> {
        self.request::<Value>(
            Method::POST,
            "/file",
            &[],
            Some(json!({
                "dir_init_lock": false,
                "dir_path": "",
                "file_name": name,
                "pdir_fid": parent_fid,
            })),
        )
        .await?;
        Ok(())
    }

    async fn resolve_dir(&self, path: &str, create: bool) -> Result<String> {
        let mut parent = self.root_fid.clone();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let files = self.list_files(&parent).await?;
            if let Some(dir) = files.iter().find(|f| !f.file && f.file_name == part) {
                parent = dir.fid.clone();
                continue;
            }
            if !create {
                bail!("directory not found: {path}");
            }
            self.mkdir(&parent, part).await?;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let files = self.list_files(&parent).await?;
            let dir = files
                .into_iter()
                .find(|f| !f.file && f.file_name == part)
                .ok_or_else(|| anyhow!("created directory did not appear: {part}"))?;
            parent = dir.fid;
        }
        Ok(parent)
    }

    async fn find_object(&self, key: &str) -> Result<Option<QuarkFile>> {
        let key = key.trim_matches('/');
        if key.is_empty() {
            return Ok(None);
        }
        let (dir, name) = split_key(key);
        let parent = match self.resolve_dir(dir, false).await {
            Ok(fid) => fid,
            Err(_) => return Ok(None),
        };
        let files = self.list_files(&parent).await?;
        Ok(files.into_iter().find(|f| f.file_name == name))
    }

    async fn download_url(&self, fid: &str) -> Result<String> {
        let resp: DownResp = self
            .request(
                Method::POST,
                "/file/download",
                &[],
                Some(json!({ "fids": [fid] })),
            )
            .await?;
        resp.data
            .first()
            .map(|d| d.download_url.clone())
            .filter(|u| !u.is_empty())
            .ok_or_else(|| anyhow!("quark did not return a download URL"))
    }

    async fn delete_fid(&self, fid: &str) -> Result<()> {
        self.request::<Value>(
            Method::POST,
            "/file/delete",
            &[],
            Some(json!({
                "action_type": 1,
                "exclude_fids": [],
                "filelist": [fid],
            })),
        )
        .await?;
        Ok(())
    }

    async fn put_object(&self, key: &str, content_type: &str, body: Bytes) -> Result<()> {
        let total_start = SystemTime::now();
        let expected_size = body.len() as i64;
        if let Some(existing) = self.find_object(key).await? {
            self.delete_fid(&existing.fid).await?;
        }

        let (dir, name) = split_key(key);
        let parent = self.resolve_dir(dir, true).await?;
        let md5_hex = format!("{:x}", md5::compute(&body));
        let sha1_hex = hex::encode(Sha1::digest(&body));
        let now = chrono_millis();

        let pre_start = SystemTime::now();
        let pre: UpPreResp = self
            .request(
                Method::POST,
                "/file/upload/pre",
                &[],
                Some(json!({
                    "ccp_hash_update": true,
                    "dir_name": "",
                    "file_name": name,
                    "format_type": content_type,
                    "l_created_at": now,
                    "l_updated_at": now,
                    "pdir_fid": parent,
                    "size": body.len(),
                })),
            )
            .await?;
        timing_log("upload.pre", key, expected_size, pre_start);
        if pre.data.finish {
            timing_log("upload.total.instant", key, expected_size, total_start);
            return Ok(());
        }

        let hash_start = SystemTime::now();
        let hash: HashResp = self
            .request(
                Method::POST,
                "/file/update/hash",
                &[],
                Some(json!({
                    "md5": md5_hex,
                    "sha1": sha1_hex,
                    "task_id": pre.data.task_id,
                })),
            )
            .await?;
        timing_log("upload.hash", key, expected_size, hash_start);
        if hash.data.finish {
            timing_log("upload.total.dedupe", key, expected_size, total_start);
            return Ok(());
        }

        let part_size = pre.metadata.part_size.max(1024 * 1024);
        let mut etags = Vec::new();
        for (idx, chunk) in body.chunks(part_size).enumerate() {
            let part_start = SystemTime::now();
            let etag = self
                .upload_part(
                    &pre.data,
                    content_type,
                    idx + 1,
                    Bytes::copy_from_slice(chunk),
                )
                .await?;
            timing_log(
                &format!("upload.part.{}", idx + 1),
                key,
                chunk.len() as i64,
                part_start,
            );
            etags.push(etag);
        }
        let commit_start = SystemTime::now();
        self.upload_commit(&pre.data, &etags).await?;
        timing_log("upload.commit", key, expected_size, commit_start);
        let finish_start = SystemTime::now();
        self.upload_finish(&pre.data).await?;
        timing_log("upload.finish", key, expected_size, finish_start);
        let visible_start = SystemTime::now();
        self.wait_until_visible(key, expected_size).await?;
        timing_log("upload.visible", key, expected_size, visible_start);
        timing_log("upload.total", key, expected_size, total_start);
        Ok(())
    }

    async fn wait_until_visible(&self, key: &str, expected_size: i64) -> Result<()> {
        let mut last = None;
        for _ in 0..20 {
            match self.find_object(key).await? {
                Some(file) if file.file && file.size == expected_size => return Ok(()),
                Some(file) => last = Some(format!("visible with size {}", file.size)),
                None => last = Some("not visible".to_string()),
            }
            sleep(Duration::from_millis(500)).await;
        }
        bail!(
            "uploaded object is not visible yet: {}",
            last.unwrap_or_else(|| "unknown".to_string())
        )
    }

    async fn upload_part(
        &self,
        pre: &UpPreData,
        content_type: &str,
        part_number: usize,
        body: Bytes,
    ) -> Result<String> {
        let date = httpdate::fmt_http_date(SystemTime::now());
        let auth_meta = format!(
            "PUT\n\n{content_type}\n{date}\nx-oss-date:{date}\nx-oss-user-agent:aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit\n/{}/{}?partNumber={part_number}&uploadId={}",
            pre.bucket, pre.obj_key, pre.upload_id
        );
        let auth: UpAuthResp = self
            .request(
                Method::POST,
                "/file/upload/auth",
                &[],
                Some(json!({
                    "auth_info": pre.auth_info,
                    "auth_meta": auth_meta,
                    "task_id": pre.task_id,
                })),
            )
            .await?;
        let url = oss_url(pre)?;
        let res = self
            .http
            .put(url)
            .query(&[
                ("partNumber", part_number.to_string()),
                ("uploadId", pre.upload_id.clone()),
            ])
            .header(header::AUTHORIZATION, auth.data.auth_key)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::REFERER, "https://pan.quark.cn/")
            .header("x-oss-date", date)
            .header(
                "x-oss-user-agent",
                "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit",
            )
            .body(body)
            .send()
            .await?;
        if !res.status().is_success() {
            bail!(
                "oss upload part failed {}: {}",
                res.status(),
                res.text().await?
            );
        }
        Ok(res
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string())
    }

    async fn upload_commit(&self, pre: &UpPreData, etags: &[String]) -> Result<()> {
        let mut xml =
            String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CompleteMultipartUpload>\n");
        for (idx, etag) in etags.iter().enumerate() {
            xml.push_str(&format!(
                "<Part>\n<PartNumber>{}</PartNumber>\n<ETag>{}</ETag>\n</Part>\n",
                idx + 1,
                etag
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");

        let content_md5 = general_purpose::STANDARD.encode(md5::compute(xml.as_bytes()).0);
        let callback = general_purpose::STANDARD.encode(serde_json::to_vec(&pre.callback)?);
        let date = httpdate::fmt_http_date(SystemTime::now());
        let auth_meta = format!(
            "POST\n{content_md5}\napplication/xml\n{date}\nx-oss-callback:{callback}\nx-oss-date:{date}\nx-oss-user-agent:aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit\n/{}/{}?uploadId={}",
            pre.bucket, pre.obj_key, pre.upload_id
        );
        let auth: UpAuthResp = self
            .request(
                Method::POST,
                "/file/upload/auth",
                &[],
                Some(json!({
                    "auth_info": pre.auth_info,
                    "auth_meta": auth_meta,
                    "task_id": pre.task_id,
                })),
            )
            .await?;
        let res = self
            .http
            .post(oss_url(pre)?)
            .query(&[("uploadId", pre.upload_id.clone())])
            .header(header::AUTHORIZATION, auth.data.auth_key)
            .header("Content-MD5", content_md5)
            .header(header::CONTENT_TYPE, "application/xml")
            .header(header::REFERER, "https://pan.quark.cn/")
            .header("x-oss-callback", callback)
            .header("x-oss-date", date)
            .header(
                "x-oss-user-agent",
                "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit",
            )
            .body(xml)
            .send()
            .await?;
        if !res.status().is_success() {
            bail!("oss commit failed {}: {}", res.status(), res.text().await?);
        }
        Ok(())
    }

    async fn upload_finish(&self, pre: &UpPreData) -> Result<()> {
        self.request::<Value>(
            Method::POST,
            "/file/upload/finish",
            &[],
            Some(json!({
                "obj_key": pre.obj_key,
                "task_id": pre.task_id,
            })),
        )
        .await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cookie = env::var("QUARK_COOKIE").context("QUARK_COOKIE is required")?;
    let root_fid = env::var("QUARK_ROOT_FID").unwrap_or_else(|_| "0".into());
    let bucket = env::var("S3_BUCKET").unwrap_or_else(|_| "quark".into());
    let max_upload_bytes = env::var("MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(128 * 1024 * 1024);
    let bind: SocketAddr = env::var("BIND")
        .unwrap_or_else(|_| "127.0.0.1:9000".into())
        .parse()?;

    let state = AppState {
        quark: QuarkClient::new(cookie, root_fid)?,
        bucket,
    };
    let app = Router::new()
        .route("/", any(root_handler))
        .route("/{bucket}", any(bucket_handler))
        .route("/{bucket}/", any(bucket_handler))
        .route("/{bucket}/{*key}", any(object_handler))
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .with_state(state);
    let listener = TcpListener::bind(bind).await?;
    info!("serving quark-s3-demo at http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root_handler(State(state): State<AppState>) -> Response {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Buckets><Bucket><Name>{}</Name></Bucket></Buckets>
</ListAllMyBucketsResult>"#,
        xml_escape(&state.bucket)
    );
    xml_response(StatusCode::OK, xml)
}

async fn bucket_handler(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    RawQuery(raw_query): RawQuery,
    method: Method,
) -> Response {
    if bucket != state.bucket {
        return s3_error(StatusCode::NOT_FOUND, "NoSuchBucket", "bucket not found");
    }
    let raw_query = raw_query.unwrap_or_default();
    if method == Method::GET && parse_query(&raw_query).contains_key("location") {
        return xml_response(
            StatusCode::OK,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<LocationConstraint xmlns="http://s3.amazonaws.com/doc/2006-03-01/">us-east-1</LocationConstraint>"#
                .to_string(),
        );
    }
    match method {
        Method::GET => list_objects(state, raw_query).await,
        Method::HEAD => StatusCode::OK.into_response(),
        Method::PUT => StatusCode::OK.into_response(),
        _ => s3_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "MethodNotAllowed",
            "unsupported method",
        ),
    }
}

async fn object_handler(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if bucket != state.bucket {
        return s3_error(StatusCode::NOT_FOUND, "NoSuchBucket", "bucket not found");
    }
    let key = percent_decode_path(&key);
    if key.trim_matches('/').is_empty() {
        return match method {
            Method::GET => list_objects(state, String::new()).await,
            Method::HEAD | Method::PUT => StatusCode::OK.into_response(),
            _ => s3_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "MethodNotAllowed",
                "unsupported method",
            ),
        };
    }
    let result = match method {
        Method::GET => get_object(&state, &key, &headers).await,
        Method::HEAD => head_object(&state, &key).await,
        Method::PUT => {
            let body = match decode_request_body(&headers, body) {
                Ok(body) => body,
                Err(err) => return s3_error_for(&err),
            };
            let etag = format!("\"{:x}\"", md5::compute(&body));
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    mime_guess::from_path(&key)
                        .first_or_octet_stream()
                        .essence_str()
                        .to_string()
                });
            state
                .quark
                .put_object(&key, &content_type, body)
                .await
                .map(|_| (StatusCode::OK, [(header::ETAG, etag)]).into_response())
        }
        Method::DELETE => delete_object(&state, &key).await,
        _ => Ok(s3_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "MethodNotAllowed",
            "unsupported method",
        )),
    };
    match result {
        Ok(resp) => resp,
        Err(err) => {
            warn!("request failed: {err:#}");
            s3_error_for(&err)
        }
    }
}

async fn list_objects(state: AppState, raw_query: String) -> Response {
    let params = parse_query(&raw_query);
    let prefix = params.get("prefix").cloned().unwrap_or_default();
    let delimiter = params.get("delimiter").cloned();
    let max_keys = params
        .get("max-keys")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1000)
        .max(1);
    let offset = params
        .get("continuation-token")
        .or_else(|| params.get("marker"))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let dir_path = if delimiter.as_deref() == Some("/") {
        prefix.trim_end_matches('/').to_string()
    } else {
        prefix.clone()
    };

    let parent = match state.quark.resolve_dir(&dir_path, false).await {
        Ok(fid) => fid,
        Err(_) => {
            return list_xml(
                &state.bucket,
                &prefix,
                delimiter.as_deref(),
                max_keys,
                None,
                Vec::new(),
                Vec::new(),
            );
        }
    };
    let recursive = delimiter.as_deref() != Some("/");
    let files = match list_files_for_s3(&state.quark, &parent, &dir_path, recursive).await {
        Ok(files) => files,
        Err(err) => return s3_error(StatusCode::BAD_GATEWAY, "QuarkError", &err.to_string()),
    };

    let mut objects = Vec::new();
    let mut common_prefixes = Vec::new();
    for (key, f) in files {
        if f.file {
            objects.push((key, f));
        } else {
            common_prefixes.push(format!("{key}/"));
        }
    }
    let total = objects.len() + common_prefixes.len();
    let next_token = if offset + max_keys < total {
        Some((offset + max_keys).to_string())
    } else {
        None
    };
    let objects_len = objects.len();
    let objects = objects
        .into_iter()
        .skip(offset)
        .take(max_keys)
        .collect::<Vec<_>>();
    let remaining = max_keys.saturating_sub(objects.len());
    let common_prefixes = common_prefixes
        .into_iter()
        .skip(offset.saturating_sub(objects_len))
        .take(remaining)
        .collect::<Vec<_>>();
    list_xml(
        &state.bucket,
        &prefix,
        delimiter.as_deref(),
        max_keys,
        next_token.as_deref(),
        objects,
        common_prefixes,
    )
}

async fn list_files_for_s3(
    quark: &QuarkClient,
    parent: &str,
    dir_path: &str,
    recursive: bool,
) -> Result<Vec<(String, QuarkFile)>> {
    let base_prefix = if dir_path.is_empty() {
        String::new()
    } else {
        format!("{}/", dir_path.trim_matches('/'))
    };
    let mut out = Vec::new();
    let mut stack = vec![(parent.to_string(), base_prefix)];
    while let Some((fid, base)) = stack.pop() {
        for f in quark.list_files(&fid).await? {
            let key = format!("{base}{}", f.file_name);
            if recursive && !f.file {
                stack.push((f.fid.clone(), format!("{key}/")));
            } else {
                out.push((key, f));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

async fn get_object(state: &AppState, key: &str, headers: &HeaderMap) -> Result<Response> {
    let total_start = SystemTime::now();
    let file = state
        .quark
        .find_object(key)
        .await?
        .filter(|f| f.file)
        .ok_or_else(|| anyhow!("object not found"))?;
    let url = state.quark.download_url(&file.fid).await?;
    let cookie = state.quark.cookie.lock().await.clone();
    let range = parse_range_header(headers, file.size)?;
    let mut req = state
        .quark
        .http
        .get(url)
        .header(header::COOKIE, cookie)
        .header(header::REFERER, REFERER)
        .header(header::USER_AGENT, QUARK_UA);
    if let Some((start, end)) = range {
        req = req.header(header::RANGE, format!("bytes={start}-{end}"));
    }
    let res = req.send().await?;
    timing_log("download.headers", key, file.size, total_start);
    let status = res.status();
    if !(status.is_success() || status == StatusCode::PARTIAL_CONTENT) {
        bail!("download failed {status}");
    }
    let stream = res.bytes_stream().map_err(std::io::Error::other);
    let mut resp = Response::new(Body::from_stream(stream));
    if let Some((start, end)) = range {
        *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
        resp.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{}", file.size))?,
        );
        resp.headers_mut().insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&(end - start + 1).to_string())?,
        );
    } else {
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut().insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&file.size.to_string())?,
        );
    }
    resp.headers_mut().insert(
        header::LAST_MODIFIED,
        HeaderValue::from_str(&http_time(file.updated_at.max(file.created_at)))?,
    );
    resp.headers_mut().insert(
        header::ACCEPT_RANGES,
        HeaderValue::from_static("bytes"),
    );
    timing_log("download.response", key, file.size, total_start);
    Ok(resp)
}

async fn head_object(state: &AppState, key: &str) -> Result<Response> {
    let file = state
        .quark
        .find_object(key)
        .await?
        .filter(|f| f.file)
        .ok_or_else(|| anyhow!("object not found"))?;
    let mut resp = StatusCode::OK.into_response();
    resp.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&file.size.to_string())?,
    );
    resp.headers_mut().insert(
        header::LAST_MODIFIED,
        HeaderValue::from_str(&http_time(file.updated_at.max(file.created_at)))?,
    );
    resp.headers_mut().insert(
        header::ACCEPT_RANGES,
        HeaderValue::from_static("bytes"),
    );
    Ok(resp)
}

async fn delete_object(state: &AppState, key: &str) -> Result<Response> {
    if let Some(file) = state.quark.find_object(key).await? {
        state.quark.delete_fid(&file.fid).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn list_xml(
    bucket: &str,
    prefix: &str,
    delimiter: Option<&str>,
    max_keys: usize,
    next_token: Option<&str>,
    objects: Vec<(String, QuarkFile)>,
    common_prefixes: Vec<String>,
) -> Response {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>{}</Name>
  <Prefix>{}</Prefix>
  <KeyCount>{}</KeyCount>
  <MaxKeys>{}</MaxKeys>
  <IsTruncated>{}</IsTruncated>
"#,
        xml_escape(bucket),
        xml_escape(prefix),
        objects.len() + common_prefixes.len(),
        max_keys,
        next_token.is_some()
    );
    if let Some(token) = next_token {
        xml.push_str(&format!(
            "  <NextContinuationToken>{}</NextContinuationToken>\n",
            xml_escape(token)
        ));
        xml.push_str(&format!(
            "  <NextMarker>{}</NextMarker>\n",
            xml_escape(token)
        ));
    }
    if let Some(delimiter) = delimiter {
        xml.push_str(&format!(
            "  <Delimiter>{}</Delimiter>\n",
            xml_escape(delimiter)
        ));
    }
    for (key, f) in objects {
        xml.push_str(&format!(
            "  <Contents><Key>{}</Key><LastModified>{}</LastModified><Size>{}</Size><StorageClass>STANDARD</StorageClass></Contents>\n",
            xml_escape(&key),
            iso_time(f.updated_at.max(f.created_at)),
            f.size
        ));
    }
    for p in common_prefixes {
        xml.push_str(&format!(
            "  <CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>\n",
            xml_escape(&p)
        ));
    }
    xml.push_str("</ListBucketResult>");
    xml_response(StatusCode::OK, xml)
}

fn xml_response(status: StatusCode, xml: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        xml,
    )
        .into_response()
}

fn s3_error(status: StatusCode, code: &str, message: &str) -> Response {
    xml_response(
        status,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>{}</Code><Message>{}</Message></Error>"#,
            xml_escape(code),
            xml_escape(message)
        ),
    )
}

fn s3_error_for(err: &anyhow::Error) -> Response {
    let message = err.to_string();
    if message.contains("object not found") {
        s3_error(StatusCode::NOT_FOUND, "NoSuchKey", "object not found")
    } else if message.contains("invalid range") {
        s3_error(
            StatusCode::RANGE_NOT_SATISFIABLE,
            "InvalidRange",
            "invalid range",
        )
    } else {
        s3_error(StatusCode::BAD_GATEWAY, "QuarkError", &message)
    }
}

fn parse_range_header(headers: &HeaderMap, size: i64) -> Result<Option<(i64, i64)>> {
    let Some(value) = headers.get(header::RANGE) else {
        return Ok(None);
    };
    let value = value.to_str()?.trim();
    let Some(spec) = value.strip_prefix("bytes=") else {
        bail!("invalid range");
    };
    let (start, end) = spec.split_once('-').ok_or_else(|| anyhow!("invalid range"))?;
    if size <= 0 {
        bail!("invalid range");
    }
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<i64>().context("invalid range")?;
        if suffix <= 0 {
            bail!("invalid range");
        }
        ((size - suffix).max(0), size - 1)
    } else {
        let start = start.parse::<i64>().context("invalid range")?;
        let end = if end.is_empty() {
            size - 1
        } else {
            end.parse::<i64>().context("invalid range")?
        };
        (start, end.min(size - 1))
    };
    if start < 0 || start >= size || end < start {
        bail!("invalid range");
    }
    Ok(Some((start, end)))
}

fn decode_request_body(headers: &HeaderMap, body: Bytes) -> Result<Bytes> {
    let is_aws_chunked = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').any(|p| p.trim().eq_ignore_ascii_case("aws-chunked")))
        .unwrap_or(false)
        || headers.contains_key("x-amz-decoded-content-length");
    if !is_aws_chunked {
        return Ok(body);
    }

    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < body.len() {
        let line_end = find_crlf(&body, pos).ok_or_else(|| anyhow!("invalid aws-chunked body"))?;
        let line = std::str::from_utf8(&body[pos..line_end])?;
        let size_hex = line
            .split(';')
            .next()
            .ok_or_else(|| anyhow!("invalid aws-chunked body"))?;
        let size = usize::from_str_radix(size_hex, 16).context("invalid aws-chunked size")?;
        pos = line_end + 2;
        if size == 0 {
            break;
        }
        if pos + size + 2 > body.len() || &body[pos + size..pos + size + 2] != b"\r\n" {
            bail!("invalid aws-chunked body");
        }
        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2;
    }

    if let Some(expected) = headers
        .get("x-amz-decoded-content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if out.len() != expected {
            bail!(
                "invalid aws-chunked decoded length: got {}, expected {}",
                out.len(),
                expected
            );
        }
    }

    Ok(Bytes::from(out))
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes[start..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .map(|idx| start + idx)
}

fn parse_query(raw: &str) -> HashMap<String, String> {
    raw.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (
                urlencoding::decode(k)
                    .unwrap_or_else(|_| k.into())
                    .into_owned(),
                urlencoding::decode(v)
                    .unwrap_or_else(|_| v.into())
                    .into_owned(),
            )
        })
        .collect()
}

fn percent_decode_path(path: &str) -> String {
    path.split('/')
        .map(|p| {
            urlencoding::decode(p)
                .unwrap_or_else(|_| p.into())
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn split_key(key: &str) -> (&str, &str) {
    let key = key.trim_matches('/');
    key.rsplit_once('/').unwrap_or(("", key))
}

fn parse_set_cookie_value(set_cookie: &str, name: &str) -> Option<String> {
    set_cookie
        .split(';')
        .next()?
        .strip_prefix(&format!("{name}="))
        .map(str::to_string)
}

fn set_cookie_value(cookie: &str, name: &str, value: &str) -> String {
    let mut found = false;
    let mut parts = Vec::new();
    for part in cookie.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        if part.starts_with(&format!("{name}=")) {
            parts.push(format!("{name}={value}"));
            found = true;
        } else {
            parts.push(part.to_string());
        }
    }
    if !found {
        parts.push(format!("{name}={value}"));
    }
    parts.join("; ")
}

fn oss_url(pre: &UpPreData) -> Result<String> {
    let host = pre
        .upload_url
        .strip_prefix("https://")
        .or_else(|| pre.upload_url.strip_prefix("http://"))
        .ok_or_else(|| anyhow!("unexpected upload_url: {}", pre.upload_url))?;
    Ok(format!("https://{}.{}/{}", pre.bucket, host, pre.obj_key))
}

fn chrono_millis() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn iso_time(millis: i64) -> String {
    let secs = (millis.max(0) / 1000) as u64;
    chrono::DateTime::<chrono::Utc>::from(
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
    )
    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn http_time(millis: i64) -> String {
    let secs = (millis.max(0) / 1000) as u64;
    httpdate::fmt_http_date(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

fn timing_log(stage: &str, key: &str, bytes: i64, start: SystemTime) {
    if env::var_os("TIMING_LOG").is_none() {
        return;
    }
    let elapsed = start.elapsed().unwrap_or_default();
    eprintln!(
        "timing stage={} ms={} bytes={} key={}",
        stage,
        elapsed.as_millis(),
        bytes,
        key
    );
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
