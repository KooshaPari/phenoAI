//! Request and trace correlation ID propagation.
//!
//! Use at operation boundaries (MCP tool calls, dispatch jobs, messaging sends).

use std::sync::Arc;
use uuid::Uuid;

/// Correlation context propagated across async/sync boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelationContext {
    pub request_id: String,
    pub trace_id: String,
}

impl CorrelationContext {
    /// Create a new context with fresh IDs.
    pub fn new() -> Self {
        Self {
            request_id: new_request_id(),
            trace_id: new_trace_id(),
        }
    }

    /// Create from an inbound request ID (generates a new trace_id).
    pub fn from_request_id(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            trace_id: new_trace_id(),
        }
    }

    /// Structured fields for tracing / JSON log `extra` payloads.
    pub fn log_fields(&self) -> [(&'static str, &str); 2] {
        [("request_id", &self.request_id), ("trace_id", &self.trace_id)]
    }
}

impl Default for CorrelationContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a short request correlation ID (12 hex chars).
pub fn request_id() -> String {
    new_request_id()
}

/// Generate a full trace ID (UUID v4).
pub fn trace_id() -> String {
    new_trace_id()
}

/// Alias for [`request_id`].
pub fn correlation_id() -> String {
    request_id()
}

fn new_request_id() -> String {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn new_trace_id() -> String {
    Uuid::new_v4().to_string()
}

/// Thread-safe holder for correlation context (e.g. per-task state).
#[derive(Debug, Clone, Default)]
pub struct CorrelationStore {
    inner: Arc<Option<CorrelationContext>>,
}

impl CorrelationStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(None),
        }
    }

    pub fn with(ctx: CorrelationContext) -> Self {
        Self {
            inner: Arc::new(Some(ctx)),
        }
    }

    pub fn get(&self) -> Option<CorrelationContext> {
        (*self.inner).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_is_twelve_chars() {
        assert_eq!(request_id().len(), 12);
    }

    #[test]
    fn trace_id_is_uuid_v4() {
        assert!(trace_id().contains('-'));
    }

    #[test]
    fn correlation_context_log_fields() {
        let ctx = CorrelationContext::from_request_id("abc123");
        let fields = ctx.log_fields();
        assert_eq!(fields[0], ("request_id", "abc123"));
        assert!(!fields[1].1.is_empty());
    }
}
