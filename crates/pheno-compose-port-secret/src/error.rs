// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-secret::error` - `SecretStoreError`.

use phenocompose_port_types::{PortError, SecretRef};
use thiserror::Error;

/// Errors a [`SecretStore`] can return.
///
/// Wraps the shared [`PortError`] taxonomy with adapter-local
/// constructors so the `?` operator works cleanly from the
/// adapter implementation without manual re-wrapping.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum SecretStoreError {
    /// The input failed validation before any backend work
    /// happened (e.g. an empty secret name).
    #[error("secret validation: {0}")]
    Validation(String),
    /// The request referred to a ref the adapter could not
    /// find (returned by [`SecretStore::get`] only —
    /// [`SecretStore::delete`] treats unknown refs as a
    /// no-op).
    #[error("secret not found: {0}")]
    NotFound(String),
    /// The underlying transport or backend failed (disk error,
    /// vault unreachable, ...).
    #[error("secret transport: {0}")]
    Transport(String),
}

impl SecretStoreError {
    /// Convenience constructor for [`SecretStoreError::Validation`].
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Convenience constructor for [`SecretStoreError::NotFound`].
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Convenience constructor for [`SecretStoreError::Transport`].
    pub fn transport(msg: impl Into<String>) -> Self {
        Self::Transport(msg.into())
    }
}

impl From<PortError> for SecretStoreError {
    fn from(e: PortError) -> Self {
        match e {
            PortError::Validation(s) | PortError::Unsupported(s) => Self::Validation(s),
            PortError::NotFound(s) => Self::NotFound(s),
            PortError::Transport(s) => Self::Transport(s),
        }
    }
}

pub(crate) fn validate_ref(r#ref: &SecretRef) -> Result<(), SecretStoreError> {
    if r#ref.name.is_empty() {
        return Err(SecretStoreError::validation(
            "secret ref name is empty",
        ));
    }
    Ok(())
}
