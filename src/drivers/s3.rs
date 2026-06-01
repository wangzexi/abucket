//! S3 mount driver.
//!
//! This is the generic object-storage backend. It expects explicit credentials
//! in config and uses path-style addressing for self-hosted or S3-like endpoints.

use serde::Deserialize;

use crate::config;
use crate::drivers::options;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(crate) struct S3Config {
    pub(crate) endpoint: String,
    pub(crate) bucket: String,
    pub(crate) region: String,
    pub(crate) access_key: String,
    pub(crate) secret_key: String,
    pub(crate) session_token: Option<String>,
    pub(crate) proxy: Option<String>,
}

pub(crate) fn from_mount(mount: &config::MountConfig) -> Option<S3Config> {
    Some(S3Config {
        endpoint: options::string(&mount.options, "endpoint")?,
        bucket: options::string(&mount.options, "bucket")?,
        region: options::string(&mount.options, "region")
            .unwrap_or_else(|| "us-east-1".to_string()),
        access_key: options::string(&mount.options, "access_key")?,
        secret_key: options::string(&mount.options, "secret_key")?,
        session_token: options::string(&mount.options, "session_token"),
        proxy: options::string(&mount.options, "proxy"),
    })
}
