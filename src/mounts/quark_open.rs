//! QuarkOpen mount.
//!
//! This mount uses Quark Drive through its open-api shape. Token refresh uses
//! the same FnNAS endpoint as OpenList's official API service:
//! `https://oauth.fnnas.com/api/v1/oauth/refreshToken`.
//!
//! Config lives in mount `options`: `refresh_token` is required; `access_token`,
//! `app_id`, `sign_key`, `refresh_url`, and `root_fid` are optional strings.
//! abucket writes refreshed OAuth state back into the same mount options in
//! `/api/config.yaml`.
//!
//! Mount config:
//! - `path`: abucket directory where Quark files appear.
//! - `root_path`: Quark directory path, such as `/` or `/backup`.
//! - `options.refresh_token`: required OAuth refresh token.
//! - `options.access_token`: optional current access token; refreshed when empty or expired.
//! - `options.app_id`: optional QuarkOpen app id; required after refresh.
//! - `options.sign_key`: optional request signing key; required after refresh.
//! - `options.refresh_url`: optional token refresh endpoint.
//! - `options.root_fid`: optional Quark folder id cache; defaults to `0`.
//!
//! OpenList reference:
//! - OpenList driver path: `drivers/quark_open`
//! - API base: `https://open-api-drive.quark.cn`
//! - signing shape: `sha256(method + "&" + pathname + "&" + timestamp_ms + "&" + sign_key)`

use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::mounts::options;
use crate::{QuarkOpenClient, QuarkOpenSharedState, config};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QuarkOpenConfig {
    /// Current OAuth access token. Can be empty when refresh can fill it.
    pub(crate) access_token: String,
    /// OAuth refresh token. This is the minimum required secret.
    pub(crate) refresh_token: String,
    /// QuarkOpen application id used as `x-pan-client-id`.
    pub(crate) app_id: String,
    /// Secret used to sign QuarkOpen requests.
    pub(crate) sign_key: String,
    /// Endpoint used to refresh OAuth tokens.
    pub(crate) refresh_url: String,
    /// Quark folder id for the configured `root_path`; `0` means account root.
    #[serde(default)]
    pub(crate) root_fid: String,
}

pub(crate) fn from_mount(mount: &config::MountConfig) -> Option<QuarkOpenConfig> {
    Some(QuarkOpenConfig {
        access_token: options::string(&mount.options, "access_token").unwrap_or_default(),
        refresh_token: options::string(&mount.options, "refresh_token")?,
        app_id: options::string(&mount.options, "app_id").unwrap_or_default(),
        sign_key: options::string(&mount.options, "sign_key").unwrap_or_default(),
        refresh_url: options::string(&mount.options, "refresh_url")
            .unwrap_or_else(|| "https://oauth.fnnas.com/api/v1/oauth/refreshToken".to_string()),
        root_fid: options::string(&mount.options, "root_fid").unwrap_or_else(|| "0".to_string()),
    })
}

pub(crate) fn client(
    config: QuarkOpenConfig,
    path: &str,
    db_path: PathBuf,
    service_config: Arc<RwLock<config::ServiceConfig>>,
    shared: Arc<QuarkOpenSharedState>,
) -> Result<QuarkOpenClient> {
    if config.refresh_token.trim().is_empty() {
        bail!("quark_open mount {path} needs options.refresh_token");
    }
    let http = Client::builder()
        .user_agent("abucket/quark-open")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;
    Ok(QuarkOpenClient {
        http,
        config: std::sync::Arc::new(tokio::sync::Mutex::new(config)),
        db_path,
        service_config,
        path: path.to_string(),
        shared,
    })
}
