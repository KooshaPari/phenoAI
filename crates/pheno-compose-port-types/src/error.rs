// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-types::error` — PortError. The single error type
//! shared by all four port-trait adapters (Composer, Publisher, Runtime,
//! SecretStore), so downstream `Box<dyn Trait>` storage can produce a
//! single `Result<_, PortError>` shape across all ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use thiserror::Error;

/// Errors that can arise in any of the port-trait
/// adapters. Each variant carries the adapter-defined
/// contextual string (typically the adapter's own error type
/// rendered via `Display`).
///
/// This is intentionally a single error type so that downstream
/// `Box<dyn Trait>` storage can produce a single `Result<_,
/// PortError>` shape across all four port traits without
/// forcing the caller to learn four error enums.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    /// The input failed validation (e.g. a [`Manifest`] with an
    /// empty name, a [`PublishTarget`] with an empty locator).
    #[error("validation error: {0}")]
    Validation(String),
    /// The request referred to a resource the adapter could not
    /// find (e.g. a container id that does not exist on
    /// `Runtime::status`).
    #[error("not found: {0}")]
    NotFound(String),
    /// The underlying transport or backend failed (network
    /// error, registry 5xx, runtime daemon offline, etc.).
    #[error("transport error: {0}")]
    Transport(String),
    /// The operation is not supported by this adapter (e.g. a
    /// `stop` on a read-only runtime). Adapters should return
    /// this rather than panicking or returning a generic
    /// `Transport` error so callers can branch on the cause.
    #[error("unsupported: {0}")]
    Unsupported(String),
}
