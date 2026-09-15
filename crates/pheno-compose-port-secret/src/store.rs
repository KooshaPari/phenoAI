// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-secret::store` - the `SecretStore` port trait.

use phenocompose_port_types::{Secret, SecretRef};
use crate::error::SecretStoreError;

/// The SecretStore port trait — `Send + Sync` + no generics + no
/// associated types ⇒ object-safe ⇒ storable as
/// `Box<dyn SecretStore>`.
pub trait SecretStore: Send + Sync {
    /// Look up the [`Secret`] at the given ref.
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError::Validation`] for inputs the
    /// adapter considers malformed (e.g. an empty name), or
    /// [`SecretStoreError::NotFound`] when no secret exists at
    /// the ref. The adapter MAY also return
    /// [`SecretStoreError::Transport`] for backend failures
    /// (disk error, vault unavailable, etc.).
    fn get(&self, r#ref: &SecretRef) -> Result<Secret, SecretStoreError>;

    /// Write the [`Secret`] to the store, returning the stored
    /// value with its (possibly bumped) `version`.
    ///
    /// Implementations MUST be atomic: a `put` either succeeds
    /// and the next `get` returns the new value, or fails and
    /// the store is unchanged. Implementations MUST bump
    /// [`Secret::version`] monotonically per ref so callers can
    /// detect concurrent updates.
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError::Validation`] for inputs the
    /// adapter considers malformed, or
    /// [`SecretStoreError::Transport`] for backend failures.
    fn put(&self, secret: &Secret) -> Result<Secret, SecretStoreError>;

    /// Remove the secret at the given ref.
    ///
    /// Idempotent: deleting a ref that does not exist is a
    /// no-op and returns `Ok(())`. Callers that need to
    /// distinguish "deleted" from "never existed" should call
    /// [`SecretStore::get`] first.
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError::Transport`] for backend
    /// failures. Validation errors are not returned here — the
    /// ref shape is checked by the adapter but the existence
    /// of the value is not required.
    fn delete(&self, r#ref: &SecretRef) -> Result<(), SecretStoreError>;

    /// List every [`SecretRef`] in the given namespace. An
    /// empty namespace means "the default scope" (adapters
    /// that don't model namespaces treat it the same as
    /// listing everything).
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError::Transport`] for backend
    /// failures.
    fn list(&self, namespace: &str) -> Result<Vec<SecretRef>, SecretStoreError>;

    /// Optional human-readable adapter name (e.g. `"memory"`,
    /// `"file"`, `"vault"`, `"noop"`). Defaults to `"unknown"`.
    fn name(&self) -> &str {
        "unknown"
    }
}

