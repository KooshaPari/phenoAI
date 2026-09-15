// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-types::secret` — SecretRef + Secret.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A strongly-typed identifier for a [`Secret`] stored by a
/// `SecretStore` port.
///
/// `SecretRef` is the addressing handle used by callers when
/// asking the port for `get` / `put` / `delete` operations. The
/// optional `namespace` field mirrors the Kubernetes-style
/// "namespace/name" convention used by the rest of the
/// PhenoCompose port types (see [`Deployment`] in the
/// orchestrator port). The `namespace` defaults to `"default"`
/// in [`SecretRef::new`]; adapters are free to interpret it
/// (or ignore it) as the underlying engine requires.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SecretRef {
    /// Optional scope qualifier (e.g. `"phenotype"`, `"default"`,
    /// `"staging"`). An empty value means "no namespace".
    pub namespace: String,
    /// The bare secret name (e.g. `"db-password"`,
    /// `"tls-certificate"`). MUST be non-empty.
    pub name: String,
}

impl SecretRef {
    /// Construct a `SecretRef` with an empty namespace and the
    /// given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            namespace: String::new(),
            name: name.into(),
        }
    }

    /// Construct a namespaced `SecretRef`.
    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// Render the ref as `"<namespace>/<name>"` (or just
    /// `"<name>"` when the namespace is empty). Useful for log
    /// lines and as a stable map key.
    pub fn locator(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.namespace, self.name)
        }
    }
}

impl AsRef<str> for SecretRef {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for SecretRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.locator())
    }
}

/// A versioned, named value stored by a `SecretStore` port.
///
/// `Secret` is the value type returned by a `get` operation and
/// the value type accepted by a `put` operation. The
/// `version` field is the adapter-defined monotonic counter
/// (vault's `version`, k8s `resourceVersion`, etc.); adapters
/// MUST bump it on every successful `put` so callers can detect
/// concurrent updates.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Secret {
    /// The address of this secret (mirrors the [`SecretRef`]
    /// passed to `get` / `put`).
    pub r#ref: SecretRef,
    /// The opaque secret material (e.g. a PEM-encoded TLS
    /// certificate, a database password, a JSON blob of API
    /// keys). Adapters MUST NOT log this value.
    pub value: String,
    /// Adapter-defined monotonic version counter. `0` is
    /// reserved for "never written"; the first successful
    /// `put` produces `version = 1`.
    pub version: u64,
}

impl Secret {
    /// Construct a `Secret` with `version = 1`. Adapters
    /// should call [`Secret::at_version`] to override the
    /// version counter.
    pub fn new(r#ref: SecretRef, value: impl Into<String>) -> Self {
        Self {
            r#ref,
            value: value.into(),
            version: 1,
        }
    }

    /// Builder-style setter for [`Secret::version`].
    #[must_use]
    pub fn at_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}
