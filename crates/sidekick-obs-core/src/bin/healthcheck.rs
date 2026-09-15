//! CLI health probe for Sidekick Rust workspace (SIDE-OPS-004).
//!
//! Usage: `cargo run -p sidekick-obs-core --bin sidekick-healthcheck`
//! Exit 0 = healthy, 1 = unhealthy.

use sidekick_obs_core::health::{ComponentCheck, HealthReport, HealthStatus, ReadinessReport};
use std::process::ExitCode;
use std::time::SystemTime;

fn main() -> ExitCode {
    let started = SystemTime::now();
    let service = std::env::var("SIDEKICK_SERVICE").unwrap_or_else(|_| "sidekick".into());
    let version = std::env::var("SIDEKICK_VERSION").unwrap_or_else(|_| "0.0.1".into());

    let core_check = ComponentCheck {
        name: "sidekick_obs_core".into(),
        status: HealthStatus::Ok,
        latency_ms: Some(0),
        detail: Some("obs-core crate loaded".into()),
    };

    let health = HealthReport::liveness(&service, &version, started);
    let readiness = ReadinessReport::from_checks(&service, vec![core_check.clone()]);

    let mode = std::env::var("SIDEKICK_HEALTH_MODE").unwrap_or_else(|_| "ready".into());
    let json = if mode == "live" {
        health.to_json()
    } else {
        readiness.to_json()
    };

    println!("{json}");

    let ok = core_check.status == HealthStatus::Ok;
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
