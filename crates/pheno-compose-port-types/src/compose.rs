// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-types::compose` — Manifest + ComposedArtifact +
//! PublishTarget + PublishReceipt. These types flow across the Composer /
//! Publisher port traits and represent the input / output surface of a
//! compose pipeline.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crate::runtime::ImageRef;

/// A composition request — describes *what* the
/// [`Composer`](crate::Composer) should produce.
///
/// `Manifest` is intentionally transport-agnostic (no file paths,
/// no URIs, no environment variables). Adapters that need to
/// resolve files or secrets pull those out of the manifest into
/// the local adapter implementation; the port type stays small.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Manifest {
    /// Human-readable name (e.g. `"phenocommand-web"`). Used by
    /// the Composer for log lines and by adapters as a default
    /// artifact name when [`Manifest::artifact_name`] is `None`.
    pub name: String,
    /// Optional explicit artifact name. If `Some`, the
    /// [`Composer`](crate::Composer) MUST use this exact string
    /// as the artifact identifier; if `None`, the composer
    /// derives one from [`Manifest::name`].
    pub artifact_name: Option<String>,
    /// Free-form key/value tags (e.g. `version=0.1.0`,
    /// `channel=stable`). Adapters MUST preserve them on the
    /// resulting [`ComposedArtifact::tags`].
    pub tags: Vec<(String, String)>,
}

impl Manifest {
    /// Construct a manifest with the given name and no explicit
    /// artifact name or tags.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            artifact_name: None,
            tags: Vec::new(),
        }
    }

    /// Builder-style setter for [`Manifest::artifact_name`].
    #[must_use]
    pub fn with_artifact_name(mut self, name: impl Into<String>) -> Self {
        self.artifact_name = Some(name.into());
        self
    }

    /// Builder-style setter for [`Manifest::tags`].
    #[must_use]
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.push((key.into(), value.into()));
        self
    }
}

/// The output of a [`Composer`](crate::Composer) — an artifact
/// ready to be [`Publisher::publish`](crate::Publisher::publish)ed
/// or [`Runtime::spawn`](crate::Runtime::spawn)ed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComposedArtifact {
    /// Stable artifact identifier (typically
    /// `<name>:<tag-digest>`).
    pub id: String,
    /// Image reference for the artifact (consumable by
    /// [`Runtime::spawn`](crate::Runtime::spawn)).
    pub image: ImageRef,
    /// Tags copied from the source [`Manifest::tags`] plus any
    /// new tags the composer wants to attach
    /// (e.g. `content-digest=sha256:...`).
    pub tags: Vec<(String, String)>,
}

impl ComposedArtifact {
    /// Construct an artifact from an id and image ref, with no
    /// tags.
    pub fn new(id: impl Into<String>, image: ImageRef) -> Self {
        Self {
            id: id.into(),
            image,
            tags: Vec::new(),
        }
    }

    /// Builder-style setter for [`ComposedArtifact::tags`].
    #[must_use]
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.push((key.into(), value.into()));
        self
    }

    /// Look up a tag by key. Returns `None` if the key is not
    /// present.
    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// A destination for a [`ComposedArtifact`] — opaque to the port
/// trait; interpreted by the concrete [`Publisher`](crate::Publisher)
/// adapter (e.g. a registry host, a local file path, a Kafka
/// topic, a `std::io::Write` sink).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublishTarget {
    /// Transport identifier (e.g. `"docker-registry"`, `"file"`,
    /// `"kafka"`). Adapters dispatch on this.
    pub kind: String,
    /// Transport-specific locator (e.g.
    /// `"registry.phenotype.internal/phenocommand/web:0.1.0"`,
    /// `"/var/lib/phenocompose/artifacts/phenocommand-web.tar"`,
    /// `"phenocommand-artifacts"`).
    pub locator: String,
}

impl PublishTarget {
    /// Construct a publish target with the given kind and
    /// locator.
    pub fn new(kind: impl Into<String>, locator: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            locator: locator.into(),
        }
    }
}

/// Proof of a successful publish — returned by
/// [`Publisher::publish`](crate::Publisher::publish).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublishReceipt {
    /// The artifact id that was published (mirrors
    /// [`ComposedArtifact::id`]).
    pub artifact_id: String,
    /// The destination that received the publish (mirrors
    /// [`PublishTarget`] but value-equal).
    pub target: PublishTarget,
    /// Adapter-defined publication locator (e.g. a digest on the
    /// remote side, a tarball path, a Kafka offset). Adapters
    /// SHOULD set this to something an operator can use to verify
    /// the publish post-hoc.
    pub published_at: String,
}

impl PublishReceipt {
    /// Construct a publish receipt.
    pub fn new(artifact_id: impl Into<String>, target: PublishTarget, published_at: impl Into<String>) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            target,
            published_at: published_at.into(),
        }
    }
}
