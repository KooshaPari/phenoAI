//! Download/fetch logic for release assets.

use std::fs;
use std::path::{Path, PathBuf};

use eidolon_core::Result;

use super::verify::verify_image;
use super::{
    effective_release_pin, guard_against_tmp, release_unavailable, ROOTFS_ASSET_SHA256_ENV,
    ROOTFS_ASSET_URL_ENV, ROOTFS_IMG_ENV, ROOTFS_RELEASE_VERSION,
};
use crate::codes;

fn env_rootfs_img() -> Option<PathBuf> {
    std::env::var_os(ROOTFS_IMG_ENV).and_then(|v| {
        let p = PathBuf::from(v);
        p.is_file().then_some(p)
    })
}

/// Resolve a local Ext4 disk without network I/O.
///
/// Order: [`ROOTFS_IMG_ENV`] (BYO, no hash) → durable cache (hash-verified when
/// an env or checkout pin is active) → fail loud.
pub fn resolve() -> Result<PathBuf> {
    resolve_with_roots(super::durable_cache_dir().ok().as_deref())
}

/// Resolve with injectable cache root (hermetic tests).
pub fn resolve_with_roots(cache_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = env_rootfs_img() {
        return Ok(p);
    }

    let pin = effective_release_pin()?.ok_or_else(|| {
        release_unavailable(
            "resolve",
            format!(
                "no Ext4 release pin — checkout defaults unpublished \
                 (ROOTFS_RELEASE_PUBLISHED={}, \
                 version={ROOTFS_RELEASE_VERSION}); set {ROOTFS_ASSET_URL_ENV}+\
                 {ROOTFS_ASSET_SHA256_ENV} after a real publish, fill checkout \
                 pin constants, set {ROOTFS_IMG_ENV}, or build with eidolon-release-rootfs \
                 (docs/guides/gh-ext4-release.md)",
                super::ROOTFS_RELEASE_PUBLISHED,
            ),
        )
    })?;

    if let Some(dir) = cache_dir {
        guard_against_tmp(dir)?;
        let img = dir.join(&pin.filename);
        if img.is_file() {
            verify_image(&img, &pin.sha256)?;
            return Ok(img);
        }
    }

    Err(release_unavailable(
        "resolve",
        format!(
            "no Ext4 release image found for pin {} (env={}) — set {ROOTFS_IMG_ENV}, \
             fetch into durable cache (`eidolon-fetch-rootfs`), or build locally",
            pin.filename, pin.from_env
        ),
    ))
}

/// Resolve, or download the pinned Ext4 disk into the durable cache (verified).
///
/// Network fetch requires feature `sandbox-rootfs-pack` and an active pin
/// (env `EIDOLON_ROOTFS_ASSET_*` or published checkout constants).
pub fn ensure() -> Result<PathBuf> {
    match resolve() {
        Ok(p) => Ok(p),
        Err(resolve_err) => {
            #[cfg(feature = "sandbox-rootfs-pack")]
            {
                let _ = &resolve_err;
                let cache = super::durable_cache_dir()?;
                fetch_into(&cache)?;
                resolve_with_roots(Some(&cache))
            }
            #[cfg(not(feature = "sandbox-rootfs-pack"))]
            {
                Err(release_unavailable(
                    "ensure",
                    format!(
                        "pin miss and feature `sandbox-rootfs-pack` off (cannot fetch): {resolve_err}"
                    ),
                ))
            }
        }
    }
}

/// Download the pinned Ext4 disk into `dest_dir`, verifying SHA-256.
#[cfg(feature = "sandbox-rootfs-pack")]
pub fn fetch_into(dest_dir: &Path) -> Result<()> {
    guard_against_tmp(dest_dir)?;
    let pin = effective_release_pin()?.ok_or_else(|| {
        release_unavailable(
            "fetch_into",
            format!(
                "no Ext4 release pin — checkout defaults unpublished; set \
                 {ROOTFS_ASSET_URL_ENV}+{ROOTFS_ASSET_SHA256_ENV} after `gh release upload`, \
                 or fill release-manifest.json + pin constants \
                 (docs/guides/gh-ext4-release.md)"
            ),
        )
    })?;
    fs::create_dir_all(dest_dir).map_err(|e| {
        release_unavailable("fetch_into", format!("mkdir {}: {e}", dest_dir.display()))
    })?;
    download_verified(&pin.url, &dest_dir.join(&pin.filename), &pin.sha256)
}

/// Parsed `https://github.com/{owner}/{repo}/releases/download/{tag}/{file}`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitHubReleaseDownload {
    owner: String,
    repo: String,
    tag: String,
    filename: String,
}

fn parse_github_release_download_url(url: &str) -> Option<GitHubReleaseDownload> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut parts = rest.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if parts.next()? != "releases" || parts.next()? != "download" {
        return None;
    }
    let tag = parts.next()?.to_string();
    let filename = parts.next()?.to_string();
    if owner.is_empty()
        || repo.is_empty()
        || tag.is_empty()
        || filename.is_empty()
        || parts.next().is_some()
    {
        return None;
    }
    Some(GitHubReleaseDownload {
        owner,
        repo,
        tag,
        filename,
    })
}

fn github_auth_token() -> Option<String> {
    std::env::var("GH_TOKEN")
        .or_else(|_| std::env::var("GITHUB_TOKEN"))
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[cfg(feature = "sandbox-rootfs-pack")]
fn curl_get_to_file(url: &str, dest: &Path, extra_headers: &[(&str, &str)]) -> Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--retry", "3", "--retry-delay", "1", "-o"])
        .arg(dest)
        .arg(url);
    for (name, value) in extra_headers {
        cmd.arg("-H").arg(format!("{name}: {value}"));
    }
    let output = cmd.output().map_err(|e| {
        release_unavailable(
            "download_verified",
            format!("curl spawn failed ({e}) — install curl or set {ROOTFS_IMG_ENV}"),
        )
    })?;
    if !output.status.success() {
        let _ = fs::remove_file(dest);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(release_unavailable(
            "download_verified",
            format!("curl GET {url} failed: {}", stderr.trim()),
        ));
    }
    Ok(())
}

/// Resolve the GitHub Releases *asset API* URL (needed for private repos).
///
/// Browser `…/releases/download/…` links 404 without session cookies on private
/// repos; `GET /repos/{owner}/{repo}/releases/assets/{id}` with
/// `Accept: application/octet-stream` + token works.
#[cfg(feature = "sandbox-rootfs-pack")]
fn github_release_asset_api_url(meta: &GitHubReleaseDownload, token: &str) -> Result<String> {
    use std::process::Command;

    let api = format!(
        "https://api.github.com/repos/{}/{}/releases/tags/{}",
        meta.owner, meta.repo, meta.tag
    );
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
        ])
        .arg(&api)
        .output()
        .map_err(|e| {
            release_unavailable(
                "download_verified",
                format!("curl spawn failed resolving release tag ({e})"),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(release_unavailable(
            "download_verified",
            format!(
                "GitHub release tag lookup failed for {}/{}@{}: {}",
                meta.owner,
                meta.repo,
                meta.tag,
                stderr.trim()
            ),
        ));
    }
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        release_unavailable(
            "download_verified",
            format!("GitHub release JSON parse failed: {e}"),
        )
    })?;
    let assets = body
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| {
            release_unavailable(
                "download_verified",
                format!("GitHub release {} has no assets array", meta.tag),
            )
        })?;
    for asset in assets {
        let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if name == meta.filename {
            if let Some(url) = asset.get("url").and_then(|u| u.as_str()) {
                return Ok(url.to_string());
            }
        }
    }
    Err(release_unavailable(
        "download_verified",
        format!(
            "GitHub release {} has no asset named {} — check tag / filename pin",
            meta.tag, meta.filename
        ),
    ))
}

#[cfg(feature = "sandbox-rootfs-pack")]
pub(crate) fn download_verified(url: &str, dest: &Path, expected_sha256: &str) -> Result<()> {
    if dest.is_file() {
        match verify_image(dest, expected_sha256) {
            Ok(()) => return Ok(()),
            Err(_) => {
                let _ = fs::remove_file(dest);
            }
        }
    }

    let tmp = dest.with_extension("img.partial");
    let _ = fs::remove_file(&tmp);

    // Public assets: plain browser URL. Private repos: browser URL 404s — use
    // Assets API + GH_TOKEN/GITHUB_TOKEN (wraps curl; mirrors UIA2 curl fetch).
    let plain = curl_get_to_file(url, &tmp, &[]);
    let downloaded = match plain {
        Ok(()) => Ok(()),
        Err(plain_err) => {
            if let (Some(meta), Some(token)) =
                (parse_github_release_download_url(url), github_auth_token())
            {
                let api_url = github_release_asset_api_url(&meta, &token)?;
                curl_get_to_file(
                    &api_url,
                    &tmp,
                    &[
                        ("Authorization", &format!("Bearer {token}")),
                        ("Accept", "application/octet-stream"),
                        ("X-GitHub-Api-Version", "2022-11-28"),
                    ],
                )
                .map_err(|api_err| {
                    release_unavailable(
                        "download_verified",
                        format!(
                            "public download failed ({plain_err}); private Assets API \
                             also failed ({api_err})"
                        ),
                    )
                })
            } else if parse_github_release_download_url(url).is_some() {
                Err(release_unavailable(
                    "download_verified",
                    format!(
                        "{plain_err}; private GitHub release assets need GH_TOKEN or \
                         GITHUB_TOKEN (or set {ROOTFS_IMG_ENV} to a local disk)"
                    ),
                ))
            } else {
                Err(plain_err)
            }
        }
    };
    if let Err(e) = downloaded {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    verify_image(&tmp, expected_sha256).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e
    })?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        release_unavailable(
            "download_verified",
            format!("rename {} → {}: {e}", tmp.display(), dest.display()),
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::codes;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_github_release_download_url_ok() {
        let m = parse_github_release_download_url(
            "https://github.com/KooshaPari/Eidolon/releases/download/rootfs-v0.1.0/\
             eidolon-canned-rootfs-0.1.0-x86_64.ext4.img",
        )
        .expect("parse");
        assert_eq!(m.owner, "KooshaPari");
        assert_eq!(m.repo, "Eidolon");
        assert_eq!(m.tag, "rootfs-v0.1.0");
        assert_eq!(m.filename, "eidolon-canned-rootfs-0.1.0-x86_64.ext4.img");
        assert!(parse_github_release_download_url("https://example.invalid/a.img").is_none());
    }

    #[cfg(feature = "sandbox-rootfs-pack")]
    #[test]
    fn resolve_uses_env_pin_cached_image() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(ROOTFS_IMG_ENV);
        let dir = super::super::staging::tests::temp_dir("env-pin-cache");
        let img = super::super::staging::tests::fake_img(&dir);
        let hex = verify::sha256_file(&img).expect("hash");
        let named = dir.join("eidolon-canned-rootfs-env-pin.ext4.img");
        fs::rename(&img, &named).unwrap();
        std::env::set_var(
            super::ROOTFS_ASSET_URL_ENV,
            "https://example.invalid/eidolon-canned-rootfs-env-pin.ext4.img",
        );
        std::env::set_var(super::ROOTFS_ASSET_SHA256_ENV, &hex);
        let got = resolve_with_roots(Some(&dir)).expect("cached env pin");
        assert_eq!(got, named);
        std::env::remove_var(super::ROOTFS_ASSET_URL_ENV);
        std::env::remove_var(super::ROOTFS_ASSET_SHA256_ENV);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_pin_uses_checkout_when_published() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(super::ROOTFS_ASSET_URL_ENV);
        std::env::remove_var(super::ROOTFS_ASSET_SHA256_ENV);
        assert!(super::super::ROOTFS_RELEASE_PUBLISHED);
        let pin = effective_release_pin().unwrap().expect("checkout pin");
        assert!(!pin.from_env);
        assert_eq!(pin.sha256, super::super::ROOTFS_RELEASE_SHA256);
        assert_eq!(pin.filename, super::super::ROOTFS_RELEASE_FILENAME);
    }

    #[test]
    fn resolve_prefers_env_override() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = super::super::staging::tests::temp_dir("env-img");
        let img = super::super::staging::tests::fake_img(&dir);
        std::env::set_var(ROOTFS_IMG_ENV, &img);
        let got = resolve_with_roots(None).expect("env");
        assert_eq!(got, img);
        std::env::remove_var(ROOTFS_IMG_ENV);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "sandbox-rootfs-pack")]
    #[test]
    fn fetch_into_fails_loud_on_incomplete_env_override() {
        let _g = ENV_LOCK.lock().unwrap();
        let out = super::super::staging::tests::temp_dir("fetch-incomplete-env");
        std::env::set_var(
            super::ROOTFS_ASSET_URL_ENV,
            "https://example.invalid/missing.img",
        );
        std::env::remove_var(super::ROOTFS_ASSET_SHA256_ENV);
        let err = fetch_into(&out).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE)
        );
        std::env::remove_var(super::ROOTFS_ASSET_URL_ENV);
        let _ = fs::remove_dir_all(&out);
    }

    #[cfg(feature = "sandbox-rootfs-pack")]
    #[test]
    fn verify_rejects_mismatch() {
        let dir = super::super::tests::temp_dir("verify");
        let img = super::super::tests::fake_img(&dir);
        let err = verify_image(
            &img,
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE)
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
