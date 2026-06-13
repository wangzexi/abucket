//! System config mount.
//!
//! This mount exposes the live abucket config as one file in the same tree used
//! by every other mount. The default path is `/api/config.yaml`.
//!
//! Mount config:
//! - `path`: exact abucket file path for the live config. It must be a file path,
//!   not `/` and not a directory.
//! - `root_path`: not used.
//! - `options`: not used.
//!
//! Moving this mount changes the config URL. Rules that grant config access must
//! target the new `path`.

use crate::config;

#[derive(Debug, Clone)]
pub(crate) struct SystemConfigTarget {
    /// Exact mounted config file path in the abucket service tree.
    pub(crate) virtual_path: String,
}

pub(crate) fn target_from_mount(
    _mount: &config::MountConfig,
    virtual_path: &str,
) -> SystemConfigTarget {
    SystemConfigTarget {
        virtual_path: virtual_path.to_string(),
    }
}
