// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-types::runtime` — ImageRef + ContainerId +
//! ContainerStatus. These types flow across the Runtime port trait.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// A reference to a container image — consumed by
/// [`Runtime::spawn`](crate::Runtime::spawn).
///
/// `ImageRef` is intentionally minimal: just a string in
/// `<repo>[:<tag>][@<digest>]` form (or whatever the underlying
/// runtime accepts). Adapters that need richer addressing (e.g.
/// a separate `tag` and `digest` field) can split the string
/// locally.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImageRef {
    /// The full image reference string, exactly as the runtime
    /// should consume it (e.g. `"phenocommand-web:0.1.0"`,
    /// `"registry.phenotype/internal/phenocommard-web@sha256:abc..."`).
    pub reference: String,
}

impl ImageRef {
    /// Construct an image ref from a reference string.
    pub fn new(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
        }
    }

    /// Convenience: build an image ref from a repo and a tag
    /// (joined as `"<repo>:<tag>"`).
    pub fn with_tag(repo: impl AsRef<str>, tag: impl AsRef<str>) -> Self {
        Self::new(format!("{}:{}", repo.as_ref(), tag.as_ref()))
    }

    /// Parse this image reference into its OCI components.
    ///
    /// Delegates to [`oci::parse`](crate::oci::parse). Returns `None`
    /// if the reference cannot be parsed.
    ///
    /// # Example
    ///
    /// ```
    /// use phenocompose_port_types::ImageRef;
    ///
    /// let r = ImageRef::new("registry.example.org/my-app:1.2.3");
    /// let parsed = r.parse_oci().unwrap();
    /// assert_eq!(parsed.repository(), "my-app");
    /// assert_eq!(parsed.tag(), Some("1.2.3"));
    /// ```
    pub fn parse_oci(&self) -> Option<crate::oci::Reference> {
        crate::oci::parse(&self.reference).ok()
    }

    /// Returns `true` if this image reference is a valid OCI
    /// reference (has at least a tag or a digest).
    ///
    /// Delegates to [`oci::is_valid`](crate::oci::is_valid).
    pub fn is_valid_oci(&self) -> bool {
        crate::oci::is_valid(&self.reference)
    }
}

impl AsRef<str> for ImageRef {
    fn as_ref(&self) -> &str {
        &self.reference
    }
}

impl From<&str> for ImageRef {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for ImageRef {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for ImageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reference)
    }
}

/// Opaque handle to a running container — returned by
/// [`Runtime::spawn`](crate::Runtime::spawn), consumed by
/// [`Runtime::stop`](crate::Runtime::stop) and
/// [`Runtime::status`](crate::Runtime::status).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerId {
    /// The runtime-assigned id (e.g. a Docker container id, a
    /// `systemd-nspawn` machine name).
    pub id: String,
}

impl ContainerId {
    /// Construct a container id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl AsRef<str> for ContainerId {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

impl From<&str> for ContainerId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for ContainerId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for ContainerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id)
    }
}

/// The state of a container, as reported by
/// [`Runtime::status`](crate::Runtime::status).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerStatus {
    /// The container is running.
    Running,
    /// The container has exited (cleanly or not — see the exit
    /// code on the adapter side if it cares).
    Exited,
    /// The container is paused (SIGSTOP'd, cgroup frozen, etc.).
    Paused,
    /// The container does not exist (the runtime no longer has
    /// any record of the id). Adapters return this for unknown
    /// ids so callers can distinguish "stopped" from "never
    /// existed".
    NotFound,
}

impl ContainerStatus {
    /// Returns `true` if the status indicates an active container
    /// ([`ContainerStatus::Running`] or [`ContainerStatus::Paused`]).
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

impl std::fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Paused => "paused",
            Self::NotFound => "not_found",
        })
    }
}
