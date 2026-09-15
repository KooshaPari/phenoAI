//! Observability primitives: correlation IDs, health/readiness, metrics hooks, log levels.
//!
//! Traces to: docs/remediation/OBSERVABILITY.md, docs/remediation/OBSERVABILITY_WIREUP.md

pub mod correlation;
pub mod health;
pub mod log_levels;
pub mod metrics;

pub use correlation::{correlation_id, trace_id, CorrelationContext};
pub use health::{HealthReport, HealthStatus, ReadinessReport, ReadinessStatus};
pub use log_levels::{default_rust_log_filter, LogLevel, SidekickLogConfig};
pub use metrics::{Counter, Histogram, MetricsRegistry, NoopMetrics};
