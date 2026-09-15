// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-pheno-config::sandbox`

//! Sandbox resource defaults and constraints.

use serde::{Deserialize, Serialize};

/// Sandbox-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum allowed length for a [`SandboxID`].
    #[serde(default = "default_max_sandbox_id_len")]
    pub max_sandbox_id_len: usize,

    /// Estimated startup time in ms for a Wasm tier instance.
    #[serde(default = "default_wasm_startup_ms")]
    pub startup_ms_wasm: u32,

    /// Estimated startup time in ms for a gVisor tier instance.
    #[serde(default = "default_gvisor_startup_ms")]
    pub startup_ms_gvisor: u32,

    /// Estimated startup time in ms for a Firecracker tier instance.
    #[serde(default = "default_firecracker_startup_ms")]
    pub startup_ms_firecracker: u32,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_sandbox_id_len: default_max_sandbox_id_len(),
            startup_ms_wasm: default_wasm_startup_ms(),
            startup_ms_gvisor: default_gvisor_startup_ms(),
            startup_ms_firecracker: default_firecracker_startup_ms(),
        }
    }
}

const fn default_max_sandbox_id_len() -> usize {
    128
}
const fn default_wasm_startup_ms() -> u32 {
    1
}
const fn default_gvisor_startup_ms() -> u32 {
    90
}
const fn default_firecracker_startup_ms() -> u32 {
    125
}
