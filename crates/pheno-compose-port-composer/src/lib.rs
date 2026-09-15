// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-composer`
//!
//! The Composer port trait — the canonical hex-architecture port
//! for converting a [`Manifest`] into a [`ComposedArtifact`].
//!
//! Adapters implement [`Composer`] to bridge to local builders
//! (`cargo build`, `npm run build`, `docker buildx build`,
//! `bazel build`, etc.) and to whatever local artifact format
//! the downstream [`Publisher`](phenocompose_port_publisher::Publisher)
//! and [`Runtime`](phenocompose_port_runtime::Runtime) adapters
//! expect. The trait is intentionally minimal: one `compose`
//! method, returning a `Result<ComposedArtifact, ComposeError>`.
//!
//! Object-safety: the trait has no associated types, no generic
//! methods, and only `&self` receivers (with `Send + Sync`
//! super-traits) so it can be stored as `Box<dyn Composer>` and
//! dispatched dynamically — the same shape used by the L3 #57
//! `pheno-plugin` registry.
//!
//! See also: [`phenocompose_port_types`] for the value types
//! ([`Manifest`], [`ComposedArtifact`], [`ImageRef`],
//! [`PortError`]) that flow across this port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use phenocompose_port_types::{ComposedArtifact, Manifest, PortError};
use std::fmt;
use thiserror::Error;

/// The Composer port trait — `Send + Sync` + no generics + no
/// associated types ⇒ object-safe ⇒ storable as
/// `Box<dyn Composer>`.
pub trait Composer: Send + Sync {
    /// Compose the given [`Manifest`] into a
    /// [`ComposedArtifact`].
    ///
    /// Implementations SHOULD treat the manifest as
    /// value-immutable: the same `&Manifest` should always
    /// produce the same artifact (modulo side effects like
    /// network fetches the adapter might perform). This
    /// invariant is what lets a downstream orchestration layer
    /// cache compositions.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::Validation`] for malformed
    /// manifests (e.g. an empty name), [`ComposeError::Backend`]
    /// for adapter-defined failures (the wrapped `String` is
    /// the adapter's own error type rendered via `Display`).
    fn compose(&self, manifest: &Manifest) -> Result<ComposedArtifact, ComposeError>;

    /// Optional human-readable adapter name (e.g. `"cargo"`,
    /// `"docker-buildx"`, `"noop"`). Defaults to `"unknown"`.
    ///
    /// Surface for log lines and diagnostics; the trait makes
    /// no behavioral decisions on this value.
    fn name(&self) -> &str {
        "unknown"
    }
}

/// Errors a [`Composer`] can return.
///
/// This is a thin wrapper around [`PortError`] that adds the
/// `#[non_exhaustive]` attribute (so we can add variants in
/// future minor versions without a SemVer break) and a private
/// `Backend` variant carrying the adapter's own error type
/// rendered as a `String`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ComposeError {
    /// The manifest failed validation before any backend work
    /// happened.
    #[error("compose validation: {0}")]
    Validation(String),
    /// The backend (cargo, npm, docker buildx, ...) failed.
    /// The wrapped `String` is the backend's own error message.
    #[error("compose backend: {0}")]
    Backend(String),
}

impl ComposeError {
    /// Convenience constructor for [`ComposeError::Validation`].
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Convenience constructor for [`ComposeError::Backend`].
    pub fn backend(msg: impl Into<String>) -> Self {
        Self::Backend(msg.into())
    }
}

impl From<PortError> for ComposeError {
    fn from(e: PortError) -> Self {
        match e {
            PortError::Validation(s) | PortError::NotFound(s) | PortError::Transport(s) => {
                Self::Backend(s)
            }
            PortError::Unsupported(s) => Self::Validation(s),
        }
    }
}

/// A trivial in-memory [`Composer`] used for tests and as a
/// default for adapters that compose nothing (e.g. a release
/// workflow that ships a pre-built artifact).
///
/// `NoopComposer` produces an artifact whose id is
/// `<manifest.name>:noop` and whose [`ImageRef`](phenocompose_port_types::ImageRef)
/// is `<manifest.name>:noop`. Useful as a stub in tests and in
/// dry-run modes.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopComposer;

impl Composer for NoopComposer {
    fn compose(&self, manifest: &Manifest) -> Result<ComposedArtifact, ComposeError> {
        if manifest.name.is_empty() {
            return Err(ComposeError::validation("manifest.name is empty"));
        }
        let artifact_id = manifest
            .artifact_name
            .clone()
            .unwrap_or_else(|| format!("{}:noop", manifest.name));
        let image_ref = manifest
            .tags
            .iter()
            .find(|(k, _)| k == "image")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| artifact_id.clone());
        let mut artifact = ComposedArtifact::new(&artifact_id, image_ref.into());
        for (k, v) in &manifest.tags {
            artifact = artifact.with_tag(k, v);
        }
        Ok(artifact)
    }

    fn name(&self) -> &str {
        "noop"
    }
}

/// A counting [`Composer`] used in tests — records how many
/// times `compose` was called, and produces deterministic
/// artifacts.
#[derive(Debug, Default)]
pub struct CountingComposer {
    /// Number of times [`Composer::compose`] has been called.
    pub call_count: std::sync::atomic::AtomicUsize,
}

impl CountingComposer {
    /// Construct a fresh `CountingComposer` with a zero call
    /// count.
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Display for CountingComposer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("counting")
    }
}

impl Composer for CountingComposer {
    fn compose(&self, manifest: &Manifest) -> Result<ComposedArtifact, ComposeError> {
        use std::sync::atomic::Ordering;
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if manifest.name.is_empty() {
            return Err(ComposeError::validation("manifest.name is empty"));
        }
        Ok(ComposedArtifact::new(
            format!("{}:counted", manifest.name),
            format!("{}:counted", manifest.name).into(),
        ))
    }

    fn name(&self) -> &str {
        "counting"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phenocompose_port_types::ImageRef;

    #[test]
    fn noop_composer_rejects_empty_name() {
        let c = NoopComposer;
        let m = Manifest::new("");
        let err = c.compose(&m).unwrap_err();
        assert!(matches!(err, ComposeError::Validation(_)));
    }

    #[test]
    fn noop_composer_uses_artifact_name_when_provided() {
        let c = NoopComposer;
        let m = Manifest::new("phenocommand-web")
            .with_artifact_name("explicit-artifact:1.0.0");
        let a = c.compose(&m).unwrap();
        assert_eq!(a.id, "explicit-artifact:1.0.0");
        assert_eq!(a.image, ImageRef::new("explicit-artifact:1.0.0"));
    }

    #[test]
    fn noop_composer_falls_back_to_name_for_artifact_id() {
        let c = NoopComposer;
        let m = Manifest::new("phenocommand-web");
        let a = c.compose(&m).unwrap();
        assert_eq!(a.id, "phenocommand-web:noop");
        assert_eq!(a.image, ImageRef::new("phenocommand-web:noop"));
    }

    #[test]
    fn noop_composer_propagates_tags() {
        let c = NoopComposer;
        let m = Manifest::new("phenocommand-web")
            .with_tag("channel", "stable")
            .with_tag("image", "registry.phenotype/phenocommand/web:0.1.0");
        let a = c.compose(&m).unwrap();
        assert_eq!(a.tag("channel"), Some("stable"));
        // the `image` tag overrides the default image ref
        assert_eq!(a.image, ImageRef::new("registry.phenotype/phenocommand/web:0.1.0"));
    }

    #[test]
    fn counting_composer_increments_call_count() {
        let c = CountingComposer::new();
        let m = Manifest::new("phenocommand-web");
        let _ = c.compose(&m).unwrap();
        let _ = c.compose(&m).unwrap();
        let _ = c.compose(&m).unwrap();
        assert_eq!(c.call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn counting_composer_produces_deterministic_artifact() {
        let c = CountingComposer::new();
        let m = Manifest::new("phenocommand-web");
        let a1 = c.compose(&m).unwrap();
        let a2 = c.compose(&m).unwrap();
        assert_eq!(a1, a2);
        assert_eq!(a1.id, "phenocommand-web:counted");
    }

    #[test]
    fn compose_error_from_port_error_dispatches() {
        let pe = PortError::Validation("bad input".to_string());
        let ce: ComposeError = pe.into();
        assert!(matches!(ce, ComposeError::Backend(_)));

        let pe = PortError::Unsupported("not supported".to_string());
        let ce: ComposeError = pe.into();
        assert!(matches!(ce, ComposeError::Validation(_)));
    }

    #[test]
    fn composer_trait_is_object_safe() {
        fn _takes_dyn(_c: &dyn Composer) {}
        // Compile-time check: Composer is object-safe (no
        // associated types, no generic methods).
    }
}
