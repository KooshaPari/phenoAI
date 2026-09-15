//! Integration tests for sidekick-obs-core (SIDE-OBS-005 … SIDE-OBS-007).

use sidekick_obs_core::{
    correlation::CorrelationContext,
    health::{HealthReport, HealthStatus, ReadinessReport},
    log_levels::{default_rust_log_filter, LogLevel},
    metrics::{InMemoryMetrics, MetricsRegistry, METRIC_DISPATCH_TOTAL},
};

#[test]
fn obs_core_correlation_exports_request_and_trace_ids() {
    let ctx = CorrelationContext::new();
    assert_eq!(ctx.request_id.len(), 12);
    assert!(ctx.trace_id.contains('-'));
}

#[test]
fn obs_core_health_liveness_json_parseable() {
    let report = HealthReport::liveness(
        "test",
        "0.0.1",
        std::time::SystemTime::now(),
    );
    let v: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(v["status"], "ok");
}

#[test]
fn obs_core_readiness_reports_not_ready_on_failure() {
    let checks = vec![sidekick_obs_core::health::ComponentCheck {
        name: "x".into(),
        status: HealthStatus::Unhealthy,
        latency_ms: None,
        detail: None,
    }];
    let r = ReadinessReport::from_checks("svc", checks);
    let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(v["status"], "notready");
}

#[test]
fn obs_core_log_level_filter_documented() {
    let f = default_rust_log_filter("sidekick-messaging", LogLevel::Info);
    assert!(f.contains("info"));
}

#[test]
fn obs_core_metrics_counter_increments() {
    let reg = InMemoryMetrics::new();
    reg.counter(METRIC_DISPATCH_TOTAL).inc_by(2);
    assert_eq!(reg.counter_value(METRIC_DISPATCH_TOTAL), 2);
}
