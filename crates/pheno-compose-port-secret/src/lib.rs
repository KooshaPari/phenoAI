// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-secret`
//!
//! The SecretStore port trait — the canonical hex-architecture
//! port for storing, retrieving, listing, and deleting
//! versioned [`Secret`](phenocompose_port_types::Secret)s
//! identified by a [`SecretRef`](phenocompose_port_types::SecretRef).
//!
//! Adapters implement [`SecretStore`] to bridge to local secret
//! backends (an in-memory `HashMap`, a JSON file on disk, a
//! Vault instance, a Kubernetes `Secret` resource, ...). The
//! trait is intentionally transport-agnostic.
//!
//! Object-safety: the trait has no associated types, no generic
//! methods, and only `&self` receivers (with `Send + Sync`
//! super-traits) so it can be stored as `Box<dyn SecretStore>`
//! and dispatched dynamically — the same shape used by the
//! sibling port crates (Composer, Publisher, Runtime).
//!
//! See also: [`phenocompose_port_types`] for the value types
//! ([`Secret`](phenocompose_port_types::Secret),
//! [`SecretRef`](phenocompose_port_types::SecretRef),
//! [`PortError`](phenocompose_port_types::PortError)) that flow
//! across this port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use phenocompose_port_types::{PortError, Secret, SecretRef};
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;

pub mod store;
pub mod error;

pub use store::SecretStore;
pub use error::SecretStoreError;
use crate::error::validate_ref;

/// A trivial [`SecretStore`] that always returns
/// "not found" / "empty list" — used as a default for adapters
/// that don't talk to a real secret backend (e.g. a dry-run
/// mode that just logs what would be stored).
///
/// `NoopSecretStore` rejects every `put` and `get` with
/// [`SecretStoreError::NotFound`] (or `Validation`, see the
/// per-method docs), and reports an empty list. `delete` is a
/// no-op.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSecretStore;

impl SecretStore for NoopSecretStore {
    fn get(&self, r#ref: &SecretRef) -> Result<Secret, SecretStoreError> {
        validate_ref(r#ref)?;
        Err(SecretStoreError::not_found(format!(
            "noop: no secret at {}",
            r#ref
        )))
    }

    fn put(&self, secret: &Secret) -> Result<Secret, SecretStoreError> {
        validate_ref(&secret.r#ref)?;
        Err(SecretStoreError::validation(
            "noop: cannot put — backend disabled",
        ))
    }

    fn delete(&self, r#ref: &SecretRef) -> Result<(), SecretStoreError> {
        validate_ref(r#ref)?;
        Ok(())
    }

    fn list(&self, _namespace: &str) -> Result<Vec<SecretRef>, SecretStoreError> {
        Ok(Vec::new())
    }

    fn name(&self) -> &str {
        "noop"
    }
}


/// An in-memory [`SecretStore`] backed by a `HashMap` under a
/// `Mutex`. Useful for tests and as a default in the DI
/// container when no persistent backend is configured.
///
/// `InMemorySecretStore` enforces:
/// - empty ref names ⇒ `Validation`
/// - `put` always succeeds and bumps the `version` monotonically
///   (per ref) starting from 1
/// - `get` returns the latest value, or `NotFound`
/// - `delete` is idempotent (removes the entry if present)
/// - `list` returns every ref in the requested namespace
///   (empty namespace matches refs whose namespace is also
///   empty — adapters that don't model namespaces can put
///   everything in the empty namespace and list with
///   `namespace = ""`)
#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    /// Underlying map: ref-locator-string → Secret. The
    /// locator string is what [`SecretRef::locator`] returns.
    inner: Mutex<HashMap<String, Secret>>,
}

impl InMemorySecretStore {
    /// Construct a fresh `InMemorySecretStore` with no stored
    /// secrets.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored secrets. Useful for assertions in
    /// tests; not part of the [`SecretStore`] trait.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("in-memory secret store mutex poisoned").len()
    }

    /// `true` if no secrets are stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for InMemorySecretStore {
    fn get(&self, r#ref: &SecretRef) -> Result<Secret, SecretStoreError> {
        validate_ref(r#ref)?;
        let guard = self.inner.lock().expect("in-memory secret store mutex poisoned");
        guard
            .get(&r#ref.locator())
            .cloned()
            .ok_or_else(|| SecretStoreError::not_found(format!("no secret at {}", r#ref)))
    }

    fn put(&self, secret: &Secret) -> Result<Secret, SecretStoreError> {
        validate_ref(&secret.r#ref)?;
        let mut guard = self.inner.lock().expect("in-memory secret store mutex poisoned");
        let key = secret.r#ref.locator();
        let next_version = match guard.get(&key) {
            Some(existing) => existing.version + 1,
            None => 1,
        };
        let stored = Secret {
            r#ref: secret.r#ref.clone(),
            value: secret.value.clone(),
            version: next_version,
        };
        guard.insert(key, stored.clone());
        Ok(stored)
    }

    fn delete(&self, r#ref: &SecretRef) -> Result<(), SecretStoreError> {
        validate_ref(r#ref)?;
        let mut guard = self.inner.lock().expect("in-memory secret store mutex poisoned");
        guard.remove(&r#ref.locator());
        Ok(())
    }

    fn list(&self, namespace: &str) -> Result<Vec<SecretRef>, SecretStoreError> {
        let guard = self.inner.lock().expect("in-memory secret store mutex poisoned");
        let mut out: Vec<SecretRef> = guard
            .values()
            .filter(|s| s.r#ref.namespace == namespace)
            .map(|s| s.r#ref.clone())
            .collect();
        // Stable order so tests can rely on it.
        out.sort_by_key(|a| a.locator());
        Ok(out)
    }

    fn name(&self) -> &str {
        "memory"
    }
}

#[cfg(test)]


mod tests {
    use super::*;
    use super::*;
    use phenocompose_port_types::{Secret, SecretRef};

    fn ref_default(name: &str) -> SecretRef {
        SecretRef::new(name)
    }

    fn ref_ns(ns: &str, name: &str) -> SecretRef {
        SecretRef::namespaced(ns, name)
    }

    #[test]
    fn noop_store_returns_not_found_on_get() {
        let s = NoopSecretStore;
        let r = ref_default("db-password");
        let err = s.get(&r).unwrap_err();
        assert!(matches!(err, SecretStoreError::NotFound(_)));
    }

    #[test]
    fn noop_store_rejects_put_with_validation() {
        let s = NoopSecretStore;
        let r = ref_default("db-password");
        let secret = Secret::new(r, "value");
        let err = s.put(&secret).unwrap_err();
        assert!(matches!(err, SecretStoreError::Validation(_)));
    }

    #[test]
    fn noop_store_delete_is_noop() {
        let s = NoopSecretStore;
        let r = ref_default("db-password");
        s.delete(&r).unwrap();
    }

    #[test]
    fn noop_store_list_is_empty() {
        let s = NoopSecretStore;
        let v = s.list("default").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn noop_store_validates_empty_name() {
        let s = NoopSecretStore;
        let r = ref_default("");
        let err = s.get(&r).unwrap_err();
        assert!(matches!(err, SecretStoreError::Validation(_)));
    }

    #[test]
    fn in_memory_store_put_then_get_round_trip() {
        let s = InMemorySecretStore::new();
        let r = ref_default("db-password");
        let stored = s.put(&Secret::new(r.clone(), "hunter2")).unwrap();
        assert_eq!(stored.version, 1);
        let got = s.get(&r).unwrap();
        assert_eq!(got.value, "hunter2");
        assert_eq!(got.version, 1);
    }

    #[test]
    fn in_memory_store_put_bumps_version_on_existing_ref() {
        let s = InMemorySecretStore::new();
        let r = ref_default("api-key");
        let v1 = s.put(&Secret::new(r.clone(), "v1")).unwrap();
        let v2 = s.put(&Secret::new(r.clone(), "v2")).unwrap();
        let v3 = s.put(&Secret::new(r.clone(), "v3")).unwrap();
        assert_eq!(v1.version, 1);
        assert_eq!(v2.version, 2);
        assert_eq!(v3.version, 3);
        let got = s.get(&r).unwrap();
        assert_eq!(got.value, "v3");
        assert_eq!(got.version, 3);
    }

    #[test]
    fn in_memory_store_get_unknown_returns_not_found() {
        let s = InMemorySecretStore::new();
        let r = ref_default("missing");
        let err = s.get(&r).unwrap_err();
        assert!(matches!(err, SecretStoreError::NotFound(_)));
    }

    #[test]
    fn in_memory_store_delete_removes_entry() {
        let s = InMemorySecretStore::new();
        let r = ref_default("ephemeral");
        s.put(&Secret::new(r.clone(), "value")).unwrap();
        assert_eq!(s.len(), 1);
        s.delete(&r).unwrap();
        assert!(s.is_empty());
        let err = s.get(&r).unwrap_err();
        assert!(matches!(err, SecretStoreError::NotFound(_)));
    }

    #[test]
    fn in_memory_store_delete_unknown_is_idempotent() {
        let s = InMemorySecretStore::new();
        let r = ref_default("never-stored");
        s.delete(&r).unwrap();
        s.delete(&r).unwrap();
    }

    #[test]
    fn in_memory_store_list_filters_by_namespace() {
        let s = InMemorySecretStore::new();
        s.put(&Secret::new(ref_default("a"), "1")).unwrap();
        s.put(&Secret::new(ref_ns("phenotype", "b"), "2")).unwrap();
        s.put(&Secret::new(ref_ns("phenotype", "c"), "3")).unwrap();
        s.put(&Secret::new(ref_ns("staging", "d"), "4")).unwrap();

        let default = s.list("").unwrap();
        assert_eq!(default.len(), 1);
        assert_eq!(default[0].name, "a");

        let phenotype = s.list("phenotype").unwrap();
        assert_eq!(phenotype.len(), 2);
        assert_eq!(phenotype[0].name, "b");
        assert_eq!(phenotype[1].name, "c");

        let staging = s.list("staging").unwrap();
        assert_eq!(staging.len(), 1);
        assert_eq!(staging[0].name, "d");

        let unknown = s.list("does-not-exist").unwrap();
        assert!(unknown.is_empty());
    }

    #[test]
    fn in_memory_store_rejects_empty_name_on_get() {
        let s = InMemorySecretStore::new();
        let err = s.get(&ref_default("")).unwrap_err();
        assert!(matches!(err, SecretStoreError::Validation(_)));
    }

    #[test]
    fn in_memory_store_rejects_empty_name_on_put() {
        let s = InMemorySecretStore::new();
        let secret = Secret::new(ref_default(""), "value");
        let err = s.put(&secret).unwrap_err();
        assert!(matches!(err, SecretStoreError::Validation(_)));
    }

    #[test]
    fn secret_store_error_from_port_error_dispatches() {
        let pe = PortError::Validation("bad".to_string());
        let sse: SecretStoreError = pe.into();
        assert!(matches!(sse, SecretStoreError::Validation(_)));

        let pe = PortError::NotFound("missing".to_string());
        let sse: SecretStoreError = pe.into();
        assert!(matches!(sse, SecretStoreError::NotFound(_)));

        let pe = PortError::Transport("net".to_string());
        let sse: SecretStoreError = pe.into();
        assert!(matches!(sse, SecretStoreError::Transport(_)));
    }

    #[test]
    fn secret_store_trait_is_object_safe() {
        fn _takes_dyn(_s: &dyn SecretStore) {}
        // Compile-time check: SecretStore is object-safe (no
        // associated types, no generic methods).
    }
}
