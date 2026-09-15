// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-secret-file-adapter::file` - the file-backed `SecretStore` impl.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::error::FileSecretStoreError;
use phenocompose_port_types::{Secret, SecretRef};
use phenocompose_port_secret::{SecretStore, SecretStoreError};

/// File-backed [`SecretStore`](phenocompose_port_secret::SecretStore)
/// adapter.
///
/// The constructor [`FileSecretStore::open`] takes a path to a
/// JSON file; the file is created (with an empty map) if it
/// does not exist yet, or loaded (and parsed) if it does. The
/// file is rewritten atomically on every `put` and `delete`.
#[derive(Debug)]
pub struct FileSecretStore {
    /// Path to the JSON file on disk.
    path: PathBuf,
    /// In-memory mirror of the on-disk map; serialized to
    /// `path` on every mutation. The key is the
    /// [`SecretRef::locator`] of the stored value.
    inner: Mutex<BTreeMap<String, Secret>>,
}

impl FileSecretStore {
    /// Open (or create) the secret store at `path`. If the
    /// file does not exist, an empty store is created and
    /// immediately flushed to disk so subsequent reads against
    /// a fresh `FileSecretStore` see a consistent file.
    ///
    /// # Errors
    ///
    /// Returns [`FileSecretStoreError::Read`] if an existing
    /// file cannot be read, or [`FileSecretStoreError::Parse`]
    /// if its contents are not valid JSON of the expected
    /// shape.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, FileSecretStoreError> {
        let path = path.into();
        let inner = if path.exists() {
            let bytes = fs::read(&path)
                .map_err(|e| FileSecretStoreError::Read(e.to_string()))?;
            if bytes.is_empty() {
                BTreeMap::new()
            } else {
                serde_json::from_slice::<BTreeMap<String, Secret>>(&bytes)
                    .map_err(|e| FileSecretStoreError::Parse(e.to_string()))?
            }
        } else {
            BTreeMap::new()
        };
        let store = Self {
            path,
            inner: Mutex::new(inner),
        };
        // Touch the file so a brand-new store is visible to
        // other processes that open the same path before the
        // first write.
        store.flush()?;
        Ok(store)
    }

    /// Path to the backing JSON file. Useful for log lines and
    /// test assertions.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Atomically write the in-memory map to disk. Used by
    /// every mutating method; exposed as `pub` so callers can
    /// force a sync from the in-memory state to disk (e.g.
    /// before another process opens the same file).
    ///
    /// # Errors
    ///
    /// Returns [`FileSecretStoreError::Write`] if the temp
    /// file cannot be written, or
    /// [`FileSecretStoreError::Rename`] if the atomic swap
    /// fails.
    pub fn flush(&self) -> Result<(), FileSecretStoreError> {
        let guard = self.inner.lock().expect("file secret store mutex poisoned");
        let bytes = serde_json::to_vec_pretty(&*guard)
            .map_err(|e| FileSecretStoreError::Write(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| FileSecretStoreError::Write(e.to_string()))?;
            f.write_all(&bytes)
                .map_err(|e| FileSecretStoreError::Write(e.to_string()))?;
            f.sync_all()
                .map_err(|e| FileSecretStoreError::Write(e.to_string()))?;
        }
        fs::rename(&tmp, &self.path)
            .map_err(|e| FileSecretStoreError::Rename(e.to_string()))?;
        Ok(())
    }
}

impl SecretStore for FileSecretStore {
    fn get(&self, r#ref: &SecretRef) -> Result<Secret, SecretStoreError> {
        if r#ref.name.is_empty() {
            return Err(SecretStoreError::validation(
                "secret ref name is empty",
            ));
        }
        let guard = self.inner.lock().expect("file secret store mutex poisoned");
        guard
            .get(&r#ref.locator())
            .cloned()
            .ok_or_else(|| SecretStoreError::not_found(format!("no secret at {}", r#ref)))
    }

    fn put(&self, secret: &Secret) -> Result<Secret, SecretStoreError> {
        if secret.r#ref.name.is_empty() {
            return Err(SecretStoreError::validation(
                "secret ref name is empty",
            ));
        }
        // The file adapter matches the in-memory store: always
        // auto-bump the version, ignoring the incoming
        // `secret.version`. The default `Secret::new(...)` gives
        // version 1; we treat that as "I don't know the current
        // version, please bump" rather than as a strict
        // optimistic-concurrency check. Callers that need
        // strict compare-and-swap can layer that on top of
        // `get` + `put` at the application layer.
        let mut guard = self.inner.lock().expect("file secret store mutex poisoned");
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
        // Persist outside the inner map's lock? No — the
        // in-memory mutex protects the BTreeMap, but we drop
        // it before flush() so any unrelated `get` waiting on
        // the lock can interleave with the disk write.
        drop(guard);
        self.flush()?;
        Ok(stored)
    }

    fn delete(&self, r#ref: &SecretRef) -> Result<(), SecretStoreError> {
        if r#ref.name.is_empty() {
            return Err(SecretStoreError::validation(
                "secret ref name is empty",
            ));
        }
        let mut guard = self.inner.lock().expect("file secret store mutex poisoned");
        guard.remove(&r#ref.locator());
        drop(guard);
        // Always flush, even if the ref wasn't there, so the
        // file is a faithful mirror of the in-memory map.
        self.flush()?;
        Ok(())
    }

    fn list(&self, namespace: &str) -> Result<Vec<SecretRef>, SecretStoreError> {
        let guard = self.inner.lock().expect("file secret store mutex poisoned");
        let out: Vec<SecretRef> = guard
            .values()
            .filter(|s| s.r#ref.namespace == namespace)
            .map(|s| s.r#ref.clone())
            .collect();
        Ok(out)
    }

    fn name(&self) -> &str {
        "file"
    }
}
