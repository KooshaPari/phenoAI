//! Structured tracing setup for Sidekick Rust crates.
//!
//! Traces to: docs/remediation/OBSERVABILITY.md, SIDE-OBS-002

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize JSON structured tracing for production / CI log aggregation.
///
/// Respects `RUST_LOG` env var; falls back to `{crate_name}={default_level}`.
pub fn init_tracing(crate_name: &str, default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{crate_name}={default_level}")));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json().with_current_span(true))
        .init();
}

/// Initialize human-readable tracing for local development.
pub fn init_tracing_plain(crate_name: &str, default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{crate_name}={default_level}")));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observability_exports_init_functions() {
        // Contract: public API exists for downstream crates to call at startup.
        let _fp_json: fn(&str, &str) = init_tracing;
        let _fp_plain: fn(&str, &str) = init_tracing_plain;
    }
}
