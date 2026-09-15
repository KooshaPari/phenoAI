// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-pheno-config::nvms`

//! NVMS driver identification labels.

use serde::{Deserialize, Serialize};

/// Labels for the NVMS driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmsConfig {
    /// Version string reported by `nvms_version()`.
    #[serde(default = "default_version")]
    pub version: String,

    /// Platform info string reported by `nvms_platform_info()`.
    #[serde(default = "default_platform")]
    pub platform: String,
}

impl Default for NvmsConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            platform: default_platform(),
        }
    }
}

fn default_version() -> String {
    "1.0.0".to_string()
}
fn default_platform() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}
