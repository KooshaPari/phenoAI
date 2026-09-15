// SPDX-License-Identifier: MIT OR Apache-2.0
//! Centralized configuration for PhenoCompose.
//!
//! Provides a layered config loader using [`figment`]:
//! 1. Hard-coded Rust defaults
//! 2. `PhenoCompose.toml` config file (optional, CWD)
//! 3. Environment variables prefixed with `PHENO_`
//!
//! # Key groupings
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`nvms`] | NVMS driver version / platform labels |
//! | [`sandbox`] | Sandbox default resources & limits |
//! | [`perf`] | Performance simulation defaults |
//! | [`gpu`] | GPU device defaults |
//!
//! # Example
//!
//! ```rust
//! use pheno_config::PhenoConfig;
//!
//! let cfg = PhenoConfig::load().expect("config loaded");
//! assert!(cfg.sandbox.max_sandbox_id_len >= 1);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use gpu::GpuConfig;
pub use nvms::NvmsConfig;
pub use perf::PerfConfig;
pub use sandbox::SandboxConfig;

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// PhenoCompose top-level configuration.
///
/// Loaded via [`PhenoConfig::load`] which merges:
/// - Hard-coded Rust defaults
/// - `PhenoCompose.toml` (optional) in the current directory
/// - Environment variables prefixed with `PHENO_`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhenoConfig {
    /// NVMS driver / platform labels.
    #[serde(default)]
    pub nvms: NvmsConfig,

    /// Sandbox defaults and resource limits.
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Performance simulation defaults.
    #[serde(default)]
    pub perf: PerfConfig,

    /// GPU device defaults.
    #[serde(default)]
    pub gpu: GpuConfig,

    /// Driver-level defaults.
    #[serde(default)]
    pub driver: DriverConfig,
}

/// Driver-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverConfig {
    /// Number of vCPUs for Firecracker instances by default.
    #[serde(default = "default_firecracker_cpus")]
    pub firecracker_default_cpus: u32,

    /// Memory in bytes for Firecracker instances by default.
    #[serde(default = "default_firecracker_memory")]
    pub firecracker_default_memory_bytes: u64,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            firecracker_default_cpus: default_firecracker_cpus(),
            firecracker_default_memory_bytes: default_firecracker_memory(),
        }
    }
}

const fn default_firecracker_cpus() -> u32 {
    2
}
const fn default_firecracker_memory() -> u64 {
    2 * 1024 * 1024 * 1024
}

// ---------------------------------------------------------------------------
// NvmsConfig
// ---------------------------------------------------------------------------

pub mod gpu;
pub mod nvms;
pub mod perf;
pub mod sandbox;

// ---------------------------------------------------------------------------
// Combined defaults — derived via #[derive(Default)] on PhenoConfig
// ---------------------------------------------------------------------------

impl PhenoConfig {
    /// Load configuration using figment's layered providers:
    ///
    /// 1. Hard-coded Rust defaults (via [`Serialized`])
    /// 2. Optional `PhenoCompose.toml` in the current directory
    /// 3. Environment variables prefixed with `PHENO_`
    ///
    /// Later providers override earlier ones.
    ///
    /// # Errors
    ///
    /// Returns [`figment::Error`] if the TOML file exists but is
    /// malformed, or if env-var parsing fails.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(Serialized::defaults(PhenoConfig::default()))
            .merge(Toml::file("PhenoCompose.toml"))
            .merge(Env::prefixed("PHENO_").split("_"))
            .extract()
            .map_err(Box::new)
    }

    /// Load configuration, panicking on load errors.
    ///
    /// Convenience for `init` / `main` contexts where a missing
    /// config file is a hard failure.
    pub fn load_or_panic() -> Self {
        Self::load().expect("PhenoConfig: failed to load (check PhenoCompose.toml or PHENO_* env vars)")
    }

    /// Return only the parsed defaults (ignores file and env
    /// sources).  Useful in tests.
    pub fn defaults_only() -> Self {
        PhenoConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Builder-style override
// ---------------------------------------------------------------------------

impl PhenoConfig {
    /// Replace the [`NvmsConfig`] section.
    #[must_use]
    pub fn with_nvms(mut self, nvms: NvmsConfig) -> Self {
        self.nvms = nvms;
        self
    }

    /// Replace the [`SandboxConfig`] section.
    #[must_use]
    pub fn with_sandbox(mut self, sandbox: SandboxConfig) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// Replace the [`PerfConfig`] section.
    #[must_use]
    pub fn with_perf(mut self, perf: PerfConfig) -> Self {
        self.perf = perf;
        self
    }

    /// Replace the [`GpuConfig`] section.
    #[must_use]
    pub fn with_gpu(mut self, gpu: GpuConfig) -> Self {
        self.gpu = gpu;
        self
    }

    /// Replace the [`DriverConfig`] section.
    #[must_use]
    pub fn with_driver(mut self, driver: DriverConfig) -> Self {
        self.driver = driver;
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Defaults -----------------------------------------------------------

    #[test]
    fn default_config_has_sane_nvms_values() {
        let cfg = PhenoConfig::default();
        assert!(!cfg.nvms.version.is_empty(), "version must not be empty");
        assert!(!cfg.nvms.platform.is_empty(), "platform must not be empty");
        assert!(cfg.nvms.platform.contains('/'), "platform should contain '/'");
    }

    #[test]
    fn default_config_has_sane_sandbox_values() {
        let cfg = PhenoConfig::default();
        assert_eq!(cfg.sandbox.max_sandbox_id_len, 128);
        assert_eq!(cfg.sandbox.startup_ms_wasm, 1);
        assert_eq!(cfg.sandbox.startup_ms_gvisor, 90);
        assert_eq!(cfg.sandbox.startup_ms_firecracker, 125);
    }

    #[test]
    fn default_config_has_sane_driver_values() {
        let cfg = PhenoConfig::default();
        assert_eq!(cfg.driver.firecracker_default_cpus, 2);
        assert_eq!(cfg.driver.firecracker_default_memory_bytes, 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn default_config_has_sane_perf_values() {
        let cfg = PhenoConfig::default();
        assert_eq!(cfg.perf.startup_time_ns, 1_000_000);
        assert_eq!(cfg.perf.memory_used_bytes, 64 * 1024 * 1024);
        assert!((cfg.perf.gpu_utilization - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn default_config_has_sane_gpu_values() {
        let cfg = PhenoConfig::default();
        assert_eq!(cfg.gpu.memory_bytes, 8 * 1024 * 1024 * 1024);
        assert_eq!(cfg.gpu.compute_units, 8);
    }

    // -- Load without file --------------------------------------------------

    #[test]
    fn load_works_without_config_file() {
        // When no PhenoCompose.toml is present, defaults should be used.
        let cfg = PhenoConfig::load().expect("load should succeed without file");
        assert_eq!(cfg.sandbox.max_sandbox_id_len, 128);
    }

    // -- Builder overrides --------------------------------------------------

    #[test]
    fn builder_overrides_sandbox() {
        let sb = sandbox::SandboxConfig {
            max_sandbox_id_len: 64,
            ..Default::default()
        };
        let cfg = PhenoConfig::default().with_sandbox(sb);
        assert_eq!(cfg.sandbox.max_sandbox_id_len, 64);
        assert_eq!(cfg.sandbox.startup_ms_wasm, 1); // unchanged
    }

    #[test]
    fn builder_overrides_driver() {
        let drv = DriverConfig {
            firecracker_default_cpus: 4,
            firecracker_default_memory_bytes: 4 * 1024 * 1024 * 1024,
        };
        let cfg = PhenoConfig::default().with_driver(drv);
        assert_eq!(cfg.driver.firecracker_default_cpus, 4);
        assert_eq!(cfg.driver.firecracker_default_memory_bytes, 4 * 1024 * 1024 * 1024);
    }

    // -- Serialization round-trip ------------------------------------------

    #[test]
    fn default_config_round_trips_via_serde() {
        let cfg = PhenoConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let deserialized: PhenoConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.sandbox.max_sandbox_id_len, cfg.sandbox.max_sandbox_id_len);
        assert_eq!(
            deserialized.driver.firecracker_default_cpus,
            cfg.driver.firecracker_default_cpus,
        );
    }
}
