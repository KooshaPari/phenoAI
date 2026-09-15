// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-secret-file-adapter`
//!
//! File-backed [`SecretStore`](phenocompose_port_secret::SecretStore)
//! adapter. Persists secrets as a single JSON document on local
//! disk; the on-disk format is a map of
//! `secret-locator -> Secret` (the [`Secret`](phenocompose_port_types::Secret)
//! value type serializes itself when `port-types` is built with
//! the `serde` feature, which this crate enables via its
//! dependency declaration).
//!
//! # On-disk format
//!
//! The file is a single JSON object whose keys are
//! [`SecretRef::locator()`](phenocompose_port_types::SecretRef::locator)
//! strings and whose values are
//! `{"ref": {...}, "value": "...", "version": N}` objects.
//! Writes are performed atomically: the new contents are first
//! written to a sibling `.tmp` file, then `rename(2)` swaps it
//! into place so a crash mid-write cannot leave the file
//! half-formed.
//!
//! # Concurrency
//!
//! The adapter enforces per-ref optimistic concurrency via the
//! [`Secret`] `version` field: a `put` whose incoming
//! `version` does not match the stored `version` is rejected
//! with [`SecretStoreError::Validation`]. Callers that don't
//! care about concurrent updates can pass `version = 0` (the
//! "create-or-overwrite" sentinel) and the adapter will set
//! the stored value to the next monotonic version.
//!
//! # Object safety
//!
//! `FileSecretStore` stores its path and the on-disk map under
//! a `Mutex`; it is `Send + Sync` and can be wrapped in
//! `Box<dyn SecretStore>` for DI.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod file;

pub use error::FileSecretStoreError;
pub use file::FileSecretStore;

#[cfg(test)]
mod tests {
    use super::*;
    use phenocompose_port_secret::{SecretStore, SecretStoreError};
    use std::path::PathBuf;
    use super::*;
    use phenocompose_port_types::{Secret, SecretRef};
    use tempfile::tempdir;
    fn path_for(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
    }
    #[test]
    fn open_creates_empty_file_when_missing() {
    let dir = tempdir().unwrap();
    let p = path_for(&dir, "secrets.json");
    assert!(!p.exists());
    let s = FileSecretStore::open(&p).unwrap();
    assert!(p.exists());
    assert!(s.list("").unwrap().is_empty());
    }
    #[test]
    fn put_then_get_round_trip_via_file() {
    let dir = tempdir().unwrap();
    let p = path_for(&dir, "secrets.json");
    let s = FileSecretStore::open(&p).unwrap();
    let r = SecretRef::new("db-password");
    s.put(&Secret::new(r.clone(), "hunter2")).unwrap();
    let got = s.get(&r).unwrap();
    assert_eq!(got.value, "hunter2");
    assert_eq!(got.version, 1);
    }
    #[test]
    fn put_persists_across_reopen() {
    let dir = tempdir().unwrap();
    let p = path_for(&dir, "secrets.json");
    // First store: write a secret and drop the handle.
    {
    let s = FileSecretStore::open(&p).unwrap();
    s.put(&Secret::new(SecretRef::new("api-key"), "v1")).unwrap();
    }
    // Second store: open the same path and read the value.
    let s2 = FileSecretStore::open(&p).unwrap();
    let got = s2.get(&SecretRef::new("api-key")).unwrap();
    assert_eq!(got.value, "v1");
    assert_eq!(got.version, 1);
    }
    #[test]
    fn put_bumps_version_on_existing_ref() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    let r = SecretRef::new("rotating");
    s.put(&Secret::new(r.clone(), "v1")).unwrap();
    s.put(&Secret::new(r.clone(), "v2")).unwrap();
    s.put(&Secret::new(r.clone(), "v3")).unwrap();
    let got = s.get(&r).unwrap();
    assert_eq!(got.value, "v3");
    assert_eq!(got.version, 3);
    }
    #[test]
    fn put_with_zero_version_acts_as_create_or_overwrite() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    let r = SecretRef::new("flexible");
    s.put(&Secret::new(r.clone(), "v1").at_version(0)).unwrap();
    s.put(&Secret::new(r.clone(), "v2").at_version(0)).unwrap();
    let got = s.get(&r).unwrap();
    assert_eq!(got.value, "v2");
    assert_eq!(got.version, 2);
    }
    #[test]
    fn get_unknown_returns_not_found() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    let err = s.get(&SecretRef::new("missing")).unwrap_err();
    assert!(matches!(err, SecretStoreError::NotFound(_)));
    }
    #[test]
    fn delete_removes_entry_and_persists() {
    let dir = tempdir().unwrap();
    let p = path_for(&dir, "s.json");
    let s = FileSecretStore::open(&p).unwrap();
    let r = SecretRef::new("ephemeral");
    s.put(&Secret::new(r.clone(), "value")).unwrap();
    s.delete(&r).unwrap();
    let err = s.get(&r).unwrap_err();
    assert!(matches!(err, SecretStoreError::NotFound(_)));
    // Reopen and confirm the delete persisted.
    let s2 = FileSecretStore::open(&p).unwrap();
    let err = s2.get(&r).unwrap_err();
    assert!(matches!(err, SecretStoreError::NotFound(_)));
    }
    #[test]
    fn delete_unknown_is_idempotent() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    s.delete(&SecretRef::new("never-stored")).unwrap();
    s.delete(&SecretRef::new("never-stored")).unwrap();
    }
    #[test]
    fn list_filters_by_namespace() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    s.put(&Secret::new(SecretRef::new("a"), "1")).unwrap();
    s.put(&Secret::new(SecretRef::namespaced("phenotype", "b"), "2")).unwrap();
    s.put(&Secret::new(SecretRef::namespaced("phenotype", "c"), "3")).unwrap();
    s.put(&Secret::new(SecretRef::namespaced("staging", "d"), "4")).unwrap();
    assert_eq!(s.list("").unwrap().len(), 1);
    assert_eq!(s.list("phenotype").unwrap().len(), 2);
    assert_eq!(s.list("staging").unwrap().len(), 1);
    assert!(s.list("none").unwrap().is_empty());
    }
    #[test]
    fn get_rejects_empty_name() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    let err = s.get(&SecretRef::new("")).unwrap_err();
    assert!(matches!(err, SecretStoreError::Validation(_)));
    }
    #[test]
    fn put_rejects_empty_name() {
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    let err = s.put(&Secret::new(SecretRef::new(""), "value")).unwrap_err();
    assert!(matches!(err, SecretStoreError::Validation(_)));
    }
    #[test]
    fn open_rejects_corrupt_json() {
    let dir = tempdir().unwrap();
    let p = path_for(&dir, "corrupt.json");
    std::fs::write(&p, b"this is not json").unwrap();
    let err = FileSecretStore::open(&p).unwrap_err();
    assert!(matches!(err, FileSecretStoreError::Parse(_)));
    }
    #[test]
    fn file_store_trait_is_object_safe() {
    fn _takes_dyn(_s: &dyn SecretStore) {}
    // Compile-time check: SecretStore is object-safe via
    // FileSecretStore.
    let dir = tempdir().unwrap();
    let s = FileSecretStore::open(path_for(&dir, "s.json")).unwrap();
    let _boxed: Box<dyn SecretStore> = Box::new(s);
    }
    }
