// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-secret-file-adapter::error` — `FileSecretStoreError`.

use phenocompose_port_secret::SecretStoreError;
use thiserror::Error;


/// Errors specific to the file-backed secret adapter that
/// don't fit cleanly into [`SecretStoreError`]. The
/// `From<FileSecretStoreError> for SecretStoreError`
/// conversion collapses everything into the port-trait error
/// taxonomy at the adapter boundary.
#[derive(Debug, Error)]
pub enum FileSecretStoreError {
    /// The on-disk JSON could not be parsed.
    #[error("invalid secrets file: {0}")]
    Parse(String),
    /// The on-disk JSON could not be written.
    #[error("secrets file write: {0}")]
    Write(String),
    /// The on-disk JSON could not be read.
    #[error("secrets file read: {0}")]
    Read(String),
    /// The on-disk JSON could not be renamed into place
    /// (atomic-write failure).
    #[error("secrets file rename: {0}")]
    Rename(String),
}


impl From<FileSecretStoreError> for SecretStoreError {
    fn from(e: FileSecretStoreError) -> Self {
        match e {
            FileSecretStoreError::Parse(s)
            | FileSecretStoreError::Read(s)
            | FileSecretStoreError::Write(s)
            | FileSecretStoreError::Rename(s) => SecretStoreError::transport(s),
        }
    }
}
