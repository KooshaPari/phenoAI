//! Log level configuration for Sidekick Rust binaries.
//!
//! Respects `RUST_LOG`; falls back to per-crate defaults documented in OBSERVABILITY.md.

use std::env;
use std::fmt;

/// Supported log levels (maps to `tracing` / `RUST_LOG` filters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Sidekick log configuration (env-driven).
#[derive(Debug, Clone)]
pub struct SidekickLogConfig {
    pub crate_name: String,
    pub default_level: LogLevel,
    pub json_output: bool,
}

impl SidekickLogConfig {
    pub fn new(crate_name: impl Into<String>, default_level: LogLevel) -> Self {
        Self {
            crate_name: crate_name.into(),
            default_level,
            json_output: env::var("SIDEKICK_LOG_JSON")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
        }
    }

    /// Effective `RUST_LOG` filter when env is unset.
    pub fn default_filter(&self) -> String {
        default_rust_log_filter(&self.crate_name, self.default_level)
    }

    /// Read level override from `SIDEKICK_LOG_LEVEL` or `RUST_LOG`.
    pub fn resolved_level(&self) -> LogLevel {
        if let Ok(v) = env::var("SIDEKICK_LOG_LEVEL") {
            if let Some(l) = LogLevel::parse(&v) {
                return l;
            }
        }
        self.default_level
    }
}

/// Build default `RUST_LOG` filter: `{crate}={level},sidekick_obs_core=warn`.
pub fn default_rust_log_filter(crate_name: &str, level: LogLevel) -> String {
    format!("{crate_name}={level},sidekick_obs_core=warn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_parse_roundtrip() {
        assert_eq!(LogLevel::parse("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse("bogus"), None);
    }

    #[test]
    fn default_filter_includes_crate() {
        let f = default_rust_log_filter("sidekick-messaging", LogLevel::Debug);
        assert!(f.contains("sidekick-messaging=debug"));
    }
}
