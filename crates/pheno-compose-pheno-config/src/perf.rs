// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-pheno-config::perf`

//! Performance simulation defaults (used by the in-process shim).

use serde::{Deserialize, Serialize};

/// Performance statistics defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfConfig {
    /// Simulated startup time in nanoseconds.
    #[serde(default = "default_startup_ns")]
    pub startup_time_ns: u64,

    /// Simulated memory used in bytes.
    #[serde(default = "default_memory_bytes")]
    pub memory_used_bytes: u64,

    /// Simulated GPU utilization (0.0 – 1.0).
    #[serde(default = "default_gpu_utilization")]
    pub gpu_utilization: f64,
}

impl Default for PerfConfig {
    fn default() -> Self {
        Self {
            startup_time_ns: default_startup_ns(),
            memory_used_bytes: default_memory_bytes(),
            gpu_utilization: default_gpu_utilization(),
        }
    }
}

const fn default_startup_ns() -> u64 {
    1_000_000
}
const fn default_memory_bytes() -> u64 {
    64 * 1024 * 1024
}
const fn default_gpu_utilization() -> f64 {
    0.0
}
