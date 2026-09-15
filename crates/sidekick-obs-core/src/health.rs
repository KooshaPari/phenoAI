//! Health and readiness probe types for Sidekick services.
//!
//! Services without HTTP can expose these via CLI (`sidekick-healthcheck`) or MCP tools.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Liveness: process is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Unhealthy,
}

/// Readiness: process can accept work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadinessStatus {
    Ready,
    NotReady,
}

/// `/health` response shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub service: String,
    pub version: String,
    pub uptime_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<ComponentCheck>>,
}

/// `/ready` response shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub status: ReadinessStatus,
    pub service: String,
    pub checks: Vec<ComponentCheck>,
}

/// Per-component probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentCheck {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl HealthReport {
    pub fn liveness(service: &str, version: &str, started_at: SystemTime) -> Self {
        Self {
            status: HealthStatus::Ok,
            service: service.to_string(),
            version: version.to_string(),
            uptime_secs: started_at.elapsed().unwrap_or_default().as_secs(),
            checks: None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"status":"unhealthy"}"#.to_string())
    }
}

impl ReadinessReport {
    pub fn from_checks(service: &str, checks: Vec<ComponentCheck>) -> Self {
        let status = if checks.iter().all(|c| c.status == HealthStatus::Ok) {
            ReadinessStatus::Ready
        } else {
            ReadinessStatus::NotReady
        };
        Self {
            status,
            service: service.to_string(),
            checks,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"status":"notready"}"#.to_string())
    }
}

/// Epoch seconds helper for startup timestamps.
pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn health_report_serializes_ok() {
        let report = HealthReport::liveness("sidekick-messaging", "0.0.1", SystemTime::now());
        let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], "sidekick-messaging");
    }

    #[test]
    fn readiness_not_ready_when_any_check_fails() {
        let checks = vec![
            ComponentCheck {
                name: "router".into(),
                status: HealthStatus::Ok,
                latency_ms: Some(12),
                detail: None,
            },
            ComponentCheck {
                name: "ledger".into(),
                status: HealthStatus::Unhealthy,
                latency_ms: None,
                detail: Some("cap exceeded".into()),
            },
        ];
        let report = ReadinessReport::from_checks("cheap-llm-mcp", checks);
        assert_eq!(report.status, ReadinessStatus::NotReady);
    }

    #[test]
    fn readiness_ready_when_all_ok() {
        let checks = vec![ComponentCheck {
            name: "lib".into(),
            status: HealthStatus::Ok,
            latency_ms: Some(1),
            detail: None,
        }];
        let report = ReadinessReport::from_checks("sidekick-messaging", checks);
        assert_eq!(report.status, ReadinessStatus::Ready);
    }
}
