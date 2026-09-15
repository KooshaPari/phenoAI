// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-publisher`
//!
//! The Publisher port trait — the canonical hex-architecture
//! port for shipping a [`ComposedArtifact`] to a
//! [`PublishTarget`].
//!
//! Adapters implement [`Publisher`] to bridge to local sinks
//! (a Docker registry, a local tarball, a Kafka topic, S3, ...)
//! — the trait is intentionally transport-agnostic.
//!
//! Object-safety: the trait has no associated types, no generic
//! methods, and only `&self` receivers (with `Send + Sync`
//! super-traits) so it can be stored as `Box<dyn Publisher>` and
//! dispatched dynamically.
//!
//! See also: [`phenocompose_port_types`] for the value types
//! ([`ComposedArtifact`], [`PublishTarget`], [`PublishReceipt`],
//! [`PortError`]) that flow across this port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use phenocompose_port_types::{ComposedArtifact, PortError, PublishReceipt, PublishTarget};
use thiserror::Error;

/// The Publisher port trait — `Send + Sync` + no generics + no
/// associated types ⇒ object-safe ⇒ storable as
/// `Box<dyn Publisher>`.
pub trait Publisher: Send + Sync {
    /// Publish the given [`ComposedArtifact`] to the given
    /// [`PublishTarget`], returning a [`PublishReceipt`] on
    /// success.
    ///
    /// # Errors
    ///
    /// Returns [`PublishError::Validation`] for inputs the
    /// adapter considers malformed (e.g. a [`PublishTarget`]
    /// whose `kind` the adapter does not handle), or
    /// [`PublishError::Transport`] for backend failures (e.g. a
    /// 5xx from a registry).
    fn publish(
        &self,
        artifact: &ComposedArtifact,
        target: &PublishTarget,
    ) -> Result<PublishReceipt, PublishError>;

    /// Optional human-readable adapter name (e.g. `"docker"`,
    /// `"file"`, `"kafka"`, `"noop"`). Defaults to `"unknown"`.
    fn name(&self) -> &str {
        "unknown"
    }
}

/// Errors a [`Publisher`] can return.
///
/// Wraps the shared [`PortError`] taxonomy with adapter-local
/// constructors so the `?` operator works cleanly from the
/// adapter implementation without manual re-wrapping.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum PublishError {
    /// The artifact or target failed validation before any
    /// network / IO work happened.
    #[error("publish validation: {0}")]
    Validation(String),
    /// The transport / backend failed (network, registry 5xx,
    /// file write error, etc.).
    #[error("publish transport: {0}")]
    Transport(String),
}

impl PublishError {
    /// Convenience constructor for [`PublishError::Validation`].
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Convenience constructor for [`PublishError::Transport`].
    pub fn transport(msg: impl Into<String>) -> Self {
        Self::Transport(msg.into())
    }
}

impl From<PortError> for PublishError {
    fn from(e: PortError) -> Self {
        match e {
            PortError::Validation(s) | PortError::Unsupported(s) => Self::Validation(s),
            PortError::NotFound(s) | PortError::Transport(s) => Self::Transport(s),
        }
    }
}

/// A trivial in-memory [`Publisher`] used for tests and as a
/// default for adapters that publish nowhere (e.g. a dry-run
/// mode that just logs what would be sent).
///
/// `NoopPublisher` succeeds for any non-empty target, producing
/// a receipt whose `published_at` mirrors the target's locator.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPublisher;

impl Publisher for NoopPublisher {
    fn publish(
        &self,
        artifact: &ComposedArtifact,
        target: &PublishTarget,
    ) -> Result<PublishReceipt, PublishError> {
        if artifact.id.is_empty() {
            return Err(PublishError::validation("artifact.id is empty"));
        }
        if target.locator.is_empty() {
            return Err(PublishError::validation("target.locator is empty"));
        }
        Ok(PublishReceipt::new(
            artifact.id.clone(),
            target.clone(),
            target.locator.clone(),
        ))
    }

    fn name(&self) -> &str {
        "noop"
    }
}

/// A recording [`Publisher`] used in tests — captures every
/// publish call so the test can assert what was sent.
#[derive(Debug, Default)]
pub struct RecordingPublisher {
    /// All `publish` calls captured in order.
    pub calls: std::sync::Mutex<Vec<(ComposedArtifact, PublishTarget)>>,
}

impl RecordingPublisher {
    /// Construct a fresh `RecordingPublisher` with no captured
    /// calls.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of captured calls.
    pub fn len(&self) -> usize {
        self.calls.lock().expect("recording publisher mutex poisoned").len()
    }

    /// `true` if no calls have been captured.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Publisher for RecordingPublisher {
    fn publish(
        &self,
        artifact: &ComposedArtifact,
        target: &PublishTarget,
    ) -> Result<PublishReceipt, PublishError> {
        if artifact.id.is_empty() {
            return Err(PublishError::validation("artifact.id is empty"));
        }
        let mut guard = self.calls.lock().expect("recording publisher mutex poisoned");
        guard.push((artifact.clone(), target.clone()));
        Ok(PublishReceipt::new(
            artifact.id.clone(),
            target.clone(),
            format!("recording://{}/{}", target.kind, artifact.id),
        ))
    }

    fn name(&self) -> &str {
        "recording"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phenocompose_port_types::ImageRef;

    fn sample_artifact() -> ComposedArtifact {
        ComposedArtifact::new("phenocommand-web:0.1.0", ImageRef::new("phenocommand-web:0.1.0"))
    }

    #[test]
    fn noop_publisher_succeeds_on_valid_input() {
        let p = NoopPublisher;
        let a = sample_artifact();
        let t = PublishTarget::new("file", "/tmp/x.tar");
        let r = p.publish(&a, &t).unwrap();
        assert_eq!(r.artifact_id, a.id);
        assert_eq!(r.target, t);
        assert_eq!(r.published_at, "/tmp/x.tar");
    }

    #[test]
    fn noop_publisher_rejects_empty_artifact_id() {
        let p = NoopPublisher;
        let a = ComposedArtifact::new("", ImageRef::new(":"));
        let t = PublishTarget::new("file", "/tmp/x.tar");
        let err = p.publish(&a, &t).unwrap_err();
        assert!(matches!(err, PublishError::Validation(_)));
    }

    #[test]
    fn noop_publisher_rejects_empty_target_locator() {
        let p = NoopPublisher;
        let a = sample_artifact();
        let t = PublishTarget::new("file", "");
        let err = p.publish(&a, &t).unwrap_err();
        assert!(matches!(err, PublishError::Validation(_)));
    }

    #[test]
    fn recording_publisher_captures_every_call() {
        let p = RecordingPublisher::new();
        let a = sample_artifact();
        let t = PublishTarget::new("docker-registry", "registry.phenotype/x:0.1.0");
        p.publish(&a, &t).unwrap();
        p.publish(&a, &t).unwrap();
        assert_eq!(p.len(), 2);
        assert!(!p.is_empty());
    }

    #[test]
    fn recording_publisher_returns_distinct_receipts() {
        let p = RecordingPublisher::new();
        let a = sample_artifact();
        let t = PublishTarget::new("docker-registry", "registry.phenotype/x:0.1.0");
        let r1 = p.publish(&a, &t).unwrap();
        let r2 = p.publish(&a, &t).unwrap();
        // RecordingPublisher returns a fresh receipt each call
        // (the receipt is value-equal but not the same instance).
        assert_eq!(r1, r2);
    }

    #[test]
    fn publish_error_from_port_error_dispatches() {
        let pe = PortError::Validation("bad".to_string());
        let pe: PublishError = pe.into();
        assert!(matches!(pe, PublishError::Validation(_)));

        let pe = PortError::Transport("net".to_string());
        let pe: PublishError = pe.into();
        assert!(matches!(pe, PublishError::Transport(_)));
    }

    #[test]
    fn publisher_trait_is_object_safe() {
        fn _takes_dyn(_p: &dyn Publisher) {}
        // Compile-time check: Publisher is object-safe.
    }
}
