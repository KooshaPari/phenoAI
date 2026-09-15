//! Docker container stats helpers (CPU % from cgroup deltas).
//!
//! # Honesty
//!
//! Docker Engine reports cumulative CPU nanoseconds plus a `precpu_stats`
//! baseline from the prior interval. When both samples are present, we apply
//! the standard Moby formula:
//!
//! `(cpu_delta / system_delta) * online_cpus * 100`
//!
//! One-shot stats without `precpu_stats`, or a zero `system_delta`, yield
//! `0.0` — documented best-effort, not a claim that the container is idle.

/// Compute container CPU percent from nanosecond deltas (Moby formula).
///
/// Returns `None` when `online_cpus` is zero or deltas are unusable.
pub fn cpu_percent_from_deltas(
    cpu_usage_total: u64,
    precpu_usage_total: u64,
    system_cpu_usage: u64,
    pre_system_cpu_usage: u64,
    online_cpus: u32,
) -> Option<f64> {
    if online_cpus == 0 {
        return None;
    }
    let cpu_delta = cpu_usage_total.saturating_sub(precpu_usage_total);
    let system_delta = system_cpu_usage.saturating_sub(pre_system_cpu_usage);
    if system_delta == 0 || cpu_delta == 0 {
        return None;
    }
    let percent = (cpu_delta as f64 / system_delta as f64) * f64::from(online_cpus) * 100.0;
    if percent.is_finite() && percent >= 0.0 {
        Some(percent)
    } else {
        None
    }
}

/// Map a Docker Engine stats payload to CPU percent (feature `sandbox-docker`).
#[cfg(feature = "sandbox-docker")]
pub fn cpu_percent_from_container_stats(stats: &bollard::models::ContainerStatsResponse) -> f64 {
    let cpu_stats = match stats.cpu_stats.as_ref() {
        Some(c) => c,
        None => return 0.0,
    };
    let precpu_stats = match stats.precpu_stats.as_ref() {
        Some(c) => c,
        None => return 0.0,
    };

    let cpu_total = cpu_stats
        .cpu_usage
        .as_ref()
        .and_then(|u| u.total_usage)
        .unwrap_or(0);
    let precpu_total = precpu_stats
        .cpu_usage
        .as_ref()
        .and_then(|u| u.total_usage)
        .unwrap_or(0);
    let system = cpu_stats.system_cpu_usage.unwrap_or(0);
    let pre_system = precpu_stats.system_cpu_usage.unwrap_or(0);

    let online_cpus = cpu_stats
        .online_cpus
        .or_else(|| {
            cpu_stats
                .cpu_usage
                .as_ref()
                .and_then(|u| u.percpu_usage.as_ref())
                .map(|v| v.len() as u32)
        })
        .unwrap_or(1)
        .max(1);

    cpu_percent_from_deltas(cpu_total, precpu_total, system, pre_system, online_cpus)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moby_formula_two_core_half_utilization() {
        // cpu_delta = 50ms, system_delta = 100ms, 2 CPUs → 100%
        let ns_50ms = 50_000_000u64;
        let ns_100ms = 100_000_000u64;
        let got = cpu_percent_from_deltas(
            ns_50ms,
            0,
            ns_100ms,
            0,
            2,
        )
        .expect("valid deltas");
        assert!((got - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_system_delta_returns_none() {
        assert!(cpu_percent_from_deltas(100, 0, 0, 0, 1).is_none());
    }

    #[test]
    fn missing_precpu_equivalent_returns_none() {
        assert!(cpu_percent_from_deltas(100, 100, 200, 100, 1).is_none());
    }

    #[test]
    fn zero_online_cpus_returns_none() {
        assert!(cpu_percent_from_deltas(100, 0, 200, 100, 0).is_none());
    }

    #[cfg(feature = "sandbox-docker")]
    #[test]
    fn container_stats_json_round_trip() {
        use bollard::models::ContainerStatsResponse;

        let json = r#"{
            "cpu_stats": {
                "cpu_usage": { "total_usage": 200000000 },
                "system_cpu_usage": 400000000,
                "online_cpus": 2
            },
            "precpu_stats": {
                "cpu_usage": { "total_usage": 100000000 },
                "system_cpu_usage": 200000000
            }
        }"#;
        let stats: ContainerStatsResponse = serde_json::from_str(json).expect("parse");
        let got = cpu_percent_from_container_stats(&stats);
        assert!((got - 100.0).abs() < f64::EPSILON);
    }
}
