// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-pheno-config::gpu`

//! GPU device defaults.

use serde::{Deserialize, Serialize};

/// GPU device configuration defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    /// Simulated GPU memory in bytes.
    #[serde(default = "default_gpu_memory_bytes")]
    pub memory_bytes: u64,

    /// Number of compute units / CUDA cores.
    #[serde(default = "default_compute_units")]
    pub compute_units: u32,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            memory_bytes: default_gpu_memory_bytes(),
            compute_units: default_compute_units(),
        }
    }
}

const fn default_gpu_memory_bytes() -> u64 {
    8 * 1024 * 1024 * 1024
}
const fn default_compute_units() -> u32 {
    8
}
