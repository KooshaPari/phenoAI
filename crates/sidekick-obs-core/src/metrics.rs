//! Metrics hook registry for Sidekick crates (Prometheus-compatible naming).
//!
//! Wire to `metrics` or `prometheus` crate in a follow-up PR; this module provides
//! the contract and a no-op implementation for library crates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Counter metric hook.
pub trait Counter: Send + Sync {
    fn inc(&self);
    fn inc_by(&self, n: u64);
    fn name(&self) -> &str;
}

/// Histogram metric hook (duration or size observations).
pub trait Histogram: Send + Sync {
    fn observe(&self, value: f64);
    fn name(&self) -> &str;
}

/// Registry for dispatch, messaging, and LLM metrics.
pub trait MetricsRegistry: Send + Sync {
    fn counter(&self, name: &str) -> Arc<dyn Counter>;
    fn histogram(&self, name: &str) -> Arc<dyn Histogram>;
}

/// Standard metric names (audit contract).
pub const METRIC_DISPATCH_TOTAL: &str = "sidekick_dispatch_total";
pub const METRIC_MESSAGING_SEND_TOTAL: &str = "sidekick_messaging_send_total";
pub const METRIC_LLM_REQUEST_DURATION: &str = "sidekick_llm_request_duration_seconds";
pub const METRIC_LLM_COST_USD: &str = "sidekick_llm_cost_usd_total";

/// No-op registry for library crates and tests.
#[derive(Debug, Default)]
pub struct NoopMetrics;

impl MetricsRegistry for NoopMetrics {
    fn counter(&self, name: &str) -> Arc<dyn Counter> {
        Arc::new(NoopCounter {
            name: name.to_string(),
        })
    }

    fn histogram(&self, name: &str) -> Arc<dyn Histogram> {
        Arc::new(NoopHistogram {
            name: name.to_string(),
        })
    }
}

#[derive(Debug)]
struct NoopCounter {
    name: String,
}

impl Counter for NoopCounter {
    fn inc(&self) {}
    fn inc_by(&self, _n: u64) {}
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
struct NoopHistogram {
    name: String,
}

impl Histogram for NoopHistogram {
    fn observe(&self, _value: f64) {}
    fn name(&self) -> &str {
        &self.name
    }
}

/// In-memory counter for integration tests and local dev.
#[derive(Debug)]
pub struct InMemoryCounter {
    name: String,
    value: AtomicU64,
}

impl InMemoryCounter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: AtomicU64::new(0),
        }
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

impl Counter for InMemoryCounter {
    fn inc(&self) {
        self.inc_by(1);
    }

    fn inc_by(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// In-memory registry for integration tests.
#[derive(Debug, Default)]
pub struct InMemoryMetrics {
    counters: Mutex<Vec<Arc<InMemoryCounter>>>,
}

impl InMemoryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn counter_value(&self, name: &str) -> u64 {
        let guard = self.counters.lock().unwrap();
        guard
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.get())
            .unwrap_or(0)
    }
}

impl MetricsRegistry for InMemoryMetrics {
    fn counter(&self, name: &str) -> Arc<dyn Counter> {
        let concrete = Arc::new(InMemoryCounter::new(name));
        self.counters.lock().unwrap().push(Arc::clone(&concrete));
        concrete
    }

    fn histogram(&self, name: &str) -> Arc<dyn Histogram> {
        Arc::new(NoopHistogram {
            name: name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_metric_names_are_documented() {
        assert!(METRIC_DISPATCH_TOTAL.starts_with("sidekick_"));
        assert!(METRIC_LLM_COST_USD.contains("cost"));
    }

    #[test]
    fn noop_counter_does_not_panic() {
        let reg = NoopMetrics;
        let c = reg.counter(METRIC_MESSAGING_SEND_TOTAL);
        c.inc();
        c.inc_by(5);
    }

    #[test]
    fn in_memory_counter_accumulates() {
        let reg = InMemoryMetrics::new();
        let c = reg.counter("test_total");
        c.inc();
        c.inc_by(2);
        assert_eq!(reg.counter_value("test_total"), 3);
    }
}
