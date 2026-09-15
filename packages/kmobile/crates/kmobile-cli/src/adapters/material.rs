//! Concrete CLI adapter implementing [`kmobile_core::ports::MaterialPort`].
//!
//! File-backed asset store: the adapter keeps a JSON manifest at
//! `<root>/<project>/assets.json` and provides CRUD operations for build
//! assets (icons, splash screens, signing keys, push certificates).
//!
//! Because the trait is `&self`, the adapter uses an interior
//! `tokio::sync::Mutex` so writes serialise through the lock. The
//! manifest is rewritten on every mutating call so the file on disk
//! always reflects the latest state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, info};

use kmobile_core::error::KMobileError;
use kmobile_core::ports::{AssetInfo, AssetKind, MaterialPort};

/// Persisted shape of the manifest file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    /// Project this manifest belongs to (None == default / global store).
    project: Option<String>,
    /// All assets currently tracked.
    assets: Vec<AssetInfo>,
}

/// File-system backed asset store.
pub struct CliMaterialAdapter {
    /// Directory under which per-project manifests live.
    root: PathBuf,
    /// Cached manifest for the "current" project (None == default).
    state: Arc<Mutex<Manifest>>,
}

impl CliMaterialAdapter {
    /// Build a new adapter rooted at the given directory. The directory
    /// is created if it doesn't already exist.
    pub async fn new<P: AsRef<Path>>(root: P) -> anyhow::Result<Self> {
        let root = root.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&root).await?;
        let manifest = Self::load_manifest(&root, None).await?;
        Ok(Self {
            root,
            state: Arc::new(Mutex::new(manifest)),
        })
    }

    /// The root directory the adapter writes manifests under.
    #[expect(dead_code)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Switch the adapter to operate on a different project. The new
    /// project's manifest is loaded from disk (or initialised empty).
    pub async fn use_project(&self, project: Option<&str>) -> anyhow::Result<()> {
        let manifest = Self::load_manifest(&self.root, project).await?;
        let mut guard = self.state.lock().await;
        *guard = manifest;
        Ok(())
    }

    /// Resolve the on-disk path for the given project's manifest.
    fn manifest_path(root: &Path, project: Option<&str>) -> PathBuf {
        match project {
            Some(name) => root.join(sanitize(name)).join("assets.json"),
            None => root.join("default").join("assets.json"),
        }
    }

    async fn load_manifest(root: &Path, project: Option<&str>) -> anyhow::Result<Manifest> {
        let path = Self::manifest_path(root, project);
        if path.exists() {
            let bytes = tokio::fs::read(&path).await?;
            let manifest: Manifest = serde_json::from_slice(&bytes)
                .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
            Ok(manifest)
        } else {
            Ok(Manifest {
                project: project.map(str::to_string),
                assets: Vec::new(),
            })
        }
    }

    async fn persist(&self, manifest: &Manifest) -> anyhow::Result<()> {
        let path = Self::manifest_path(&self.root, manifest.project.as_deref());
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        tokio::fs::write(&path, bytes).await?;
        Ok(())
    }

    fn next_id(&self, manifest: &Manifest) -> String {
        format!("asset-{:04}", manifest.assets.len() + 1)
    }
}

/// Replace path separators and other characters that would be unsafe in
/// a directory name.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[async_trait]
impl MaterialPort for CliMaterialAdapter {
    async fn list_assets(&self, project: Option<&str>) -> Result<Vec<AssetInfo>, KMobileError> {
        let manifest = Self::load_manifest(&self.root, project)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        Ok(manifest.assets)
    }

    async fn add_asset(
        &self,
        project: Option<&str>,
        kind: AssetKind,
        path: PathBuf,
    ) -> Result<AssetInfo, KMobileError> {
        if path.as_os_str().is_empty() {
            return Err(KMobileError::InvalidInput(
                "asset path must not be empty".into(),
            ));
        }
        let size_bytes = std::fs::metadata(&path)
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?
            .len();

        // If the caller asked for a different project than the one we
        // currently hold, reload that project's manifest first.
        {
            let guard = self.state.lock().await;
            if guard.project.as_deref() != project {
                drop(guard);
                self.use_project(project)
                    .await
                    .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
            }
        }

        let mut guard = self.state.lock().await;
        let id = self.next_id(&guard);
        let info = AssetInfo {
            id: id.clone(),
            kind,
            path: path.clone(),
            size_bytes,
        };
        guard.assets.push(info.clone());
        self.persist(&guard)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        info!(
            "Added asset id={id} kind={:?} path={}",
            info.kind,
            info.path.display()
        );
        Ok(info)
    }

    async fn remove_asset(&self, project: Option<&str>, id: &str) -> Result<bool, KMobileError> {
        {
            let guard = self.state.lock().await;
            if guard.project.as_deref() != project {
                drop(guard);
                self.use_project(project)
                    .await
                    .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
            }
        }

        let mut guard = self.state.lock().await;
        let before = guard.assets.len();
        guard.assets.retain(|a| a.id != id);
        let removed = guard.assets.len() != before;
        if removed {
            self.persist(&guard)
                .await
                .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
            debug!("Removed asset {id}");
        }
        Ok(removed)
    }

    async fn get_asset(
        &self,
        project: Option<&str>,
        id: &str,
    ) -> Result<Option<AssetInfo>, KMobileError> {
        let manifest = Self::load_manifest(&self.root, project)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        Ok(manifest.assets.into_iter().find(|a| a.id == id))
    }
}
