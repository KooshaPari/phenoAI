// SPDX-License-Identifier: MIT OR Apache-2.0
//! OCI (Open Container Initiative) image reference helpers.
//!
//! This module provides parsing, validation, and construction
//! utilities for OCI image references (the `<registry>/<repository>:<tag>`
//! and `<registry>/<repository>@<digest>` formats defined by the
//! [OCI Image Spec](https://github.com/opencontainers/image-spec).
//!
//! # Canonical home
//!
//! These types and functions originated from deduplication work
//! across the PhenoCompose port crates. The **canonical** home for
//! OCI helpers in the Phenotype ecosystem is the
//! **`phenotype-types`** crate (hosted at
//! `https://github.com/kooshapari/phenotype-types`). When that crate
//! is available as a dependency, consumers SHOULD prefer it over
//! this local module. This module exists as a transitional shim so
//! that existing port crates can migrate incrementally.
//!
//! # Examples
//!
//! ```
//! use phenocompose_port_types::oci;
//!
//! let parsed = oci::parse("registry.example.org/my-app:1.2.3").unwrap();
//! assert_eq!(parsed.registry(), Some("registry.example.org"));
//! assert_eq!(parsed.repository(), "my-app");
//! assert_eq!(parsed.tag(), Some("1.2.3"));
//! assert!(parsed.digest().is_none());
//! assert_eq!(parsed.to_string(), "registry.example.org/my-app:1.2.3");
//! ```

/// A parsed OCI image reference.
///
/// Breaks a reference string into its structural components:
///
/// ```text
/// [registry/]repository[:tag][@digest]
/// ```
///
/// | Component    | Example                                      | Required |
/// |--------------|----------------------------------------------|----------|
/// | registry     | `docker.io`, `registry.example.org:5000`     | No       |
/// | repository   | `library/ubuntu`, `my-app`                   | Yes      |
/// | tag          | `latest`, `1.2.3`                            | No       |
/// | digest       | `sha256:abc...`                              | No       |
///
/// At least one of `tag` or `digest` MUST be present for the
/// reference to be considered fully valid (see [`Reference::is_valid`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reference {
    /// Optional registry host (with optional port).
    registry: Option<String>,
    /// Repository path (e.g. `"library/ubuntu"` or `"my-app"`).
    repository: String,
    /// Optional tag (e.g. `"latest"`, `"1.2.3"`).
    tag: Option<String>,
    /// Optional digest (e.g. `"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"`).
    digest: Option<String>,
}

impl Reference {
    /// The optional registry host portion.
    pub fn registry(&self) -> Option<&str> {
        self.registry.as_deref()
    }

    /// The repository path (always present).
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// The optional tag.
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    /// The optional digest.
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }

    /// Returns `true` if at least one of `tag` or `digest` is present.
    ///
    /// Per the OCI spec, a reference should have either a tag or a
    /// digest (or both) to be meaningful. Bare repository names
    /// without any tag or digest are considered invalid for most
    /// runtime operations.
    pub fn is_valid(&self) -> bool {
        self.tag.is_some() || self.digest.is_some()
    }

    /// Reconstruct the full reference string.
    ///
    /// The output follows the standard OCI convention:
    /// `<registry>/<repository>:<tag>` or
    /// `<registry>/<repository>@<digest>` (or both, separated
    /// by both `:` and `@` if both are present).
    pub fn to_string(&self) -> String {
        let mut s = String::new();
        if let Some(reg) = &self.registry {
            s.push_str(reg);
            s.push('/');
        }
        s.push_str(&self.repository);
        if let Some(t) = &self.tag {
            s.push(':');
            s.push_str(t);
        }
        if let Some(d) = &self.digest {
            s.push('@');
            s.push_str(d);
        }
        s
    }
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string())
    }
}

/// Parse an OCI image reference string into its components.
///
/// Supports the following forms:
///
/// * `<repository>:<tag>` — e.g. `"ubuntu:22.04"`
/// * `<repository>@<digest>` — e.g. `"ubuntu@sha256:abc..."`
/// * `<registry>/<repository>:<tag>` — e.g. `"docker.io/library/ubuntu:22.04"`
/// * `<registry>/<repository>@<digest>` — e.g. `"quay.io/prometheus/node-exporter@sha256:abc..."`
/// * `<registry>/<repository>:<tag>@<digest>` — both tag and digest
/// * `<registry>:<port>/<repository>:<tag>` — registry with explicit port
///
/// # Errors
///
/// Returns an error description if the reference cannot be parsed
/// (e.g. empty string, missing repository, or malformed digest
/// prefix).
pub fn parse(reference: &str) -> Result<Reference, String> {
    let s = reference.trim();
    if s.is_empty() {
        return Err("OCI reference is empty".to_string());
    }

    // --- Split off digest --------------------------------------------------
    let (without_digest, digest) = match s.split_once('@') {
        Some((left, right)) => {
            if right.is_empty() {
                return Err("OCI reference has empty digest".to_string());
            }
            // Digest must start with a known algorithm prefix.
            let alg_prefix = right.split_once(':').map(|(a, _)| a).unwrap_or("");
            if alg_prefix.is_empty() {
                return Err(format!(
                    "OCI digest must be in <algorithm>:<hex> form, got: \"{right}\""
                ));
            }
            (left, Some(right.to_string()))
        }
        None => (s, None),
    };

    // --- Split off tag -----------------------------------------------------
    // The tag is everything after the LAST colon in the repository portion.
    // We need to be careful about registry:port patterns.
    let (repo_part, tag) = split_tag(without_digest)?;

    // --- Split registry from repository -----------------------------------
    let (registry, repository) = split_registry(repo_part);

    Ok(Reference {
        registry,
        repository,
        tag,
        digest,
    })
}

/// Split the tag from a `<registry>/<repo>:<tag>` or `<repo>:<tag>` string.
///
/// Returns `(rest, optional_tag)`. The "rest" still contains the
/// registry portion (if any) — call [`split_registry`] next.
fn split_tag(input: &str) -> Result<(&str, Option<String>), String> {
    if input.is_empty() {
        return Err("OCI reference has empty repository portion".to_string());
    }

    // We find the LAST colon that looks like a tag separator (not a
    // port number after a registry hostname). The heuristic:
    // if there's a '/' in the string, only the part after the last '/'
    // is checked for a colon (so `registry:5000/repo:tag` is parsed
    // correctly as registry=`registry:5000`, repo=`repo`, tag=`tag`).
    let after_slash = input.rsplit_once('/').map(|(_, after)| after).unwrap_or(input);

    match after_slash.rsplit_once(':') {
        Some((_, after)) if !after.is_empty() && !after.contains('/') => {
            let before_colon = &input[..input.len() - after.len() - 1];
            Ok((before_colon, Some(after.to_string())))
        }
        _ => Ok((input, None)),
    }
}

/// Split the registry host from a `<registry>/<repository>` string.
fn split_registry(input: &str) -> (Option<String>, String) {
    match input.split_once('/') {
        Some((potential_host, rest)) if potential_host.contains('.') || potential_host.contains(':') => {
            (Some(potential_host.to_string()), rest.to_string())
        }
        _ => (None, input.to_string()),
    }
}

/// Validate that an OCI image reference string is well-formed.
///
/// A valid reference must parse successfully *and* have at least
/// a tag or a digest. Bare repository names (e.g. `"ubuntu"`)
/// parse structurally but are not considered valid for runtime
/// operations.
///
/// This is a convenience wrapper around [`parse`] that discards
/// the parsed components and returns a boolean.
pub fn is_valid(reference: &str) -> bool {
    parse(reference).map_or(false, |r| r.is_valid())
}

/// Construct a canonical `<registry>/<repository>:<tag>` reference.
///
/// If `registry` is empty/None, the reference is `<repository>:<tag>`.
pub fn with_tag(registry: Option<&str>, repository: &str, tag: &str) -> String {
    let mut s = String::new();
    if let Some(reg) = registry {
        s.push_str(reg);
        s.push('/');
    }
    s.push_str(repository);
    s.push(':');
    s.push_str(tag);
    s
}

/// Construct a canonical `<registry>/<repository>@<digest>` reference.
///
/// If `registry` is empty/None, the reference is `<repository>@<digest>`.
pub fn with_digest(registry: Option<&str>, repository: &str, digest: &str) -> String {
    let mut s = String::new();
    if let Some(reg) = registry {
        s.push_str(reg);
        s.push('/');
    }
    s.push_str(repository);
    s.push('@');
    s.push_str(digest);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse -------------------------------------------------------------

    #[test]
    fn parse_simple_repo_tag() {
        let r = parse("ubuntu:22.04").unwrap();
        assert_eq!(r.registry(), None);
        assert_eq!(r.repository(), "ubuntu");
        assert_eq!(r.tag(), Some("22.04"));
        assert_eq!(r.digest(), None);
        assert!(r.is_valid());
    }

    #[test]
    fn parse_registry_repo_tag() {
        let r = parse("registry.example.org/my-app:1.2.3").unwrap();
        assert_eq!(r.registry(), Some("registry.example.org"));
        assert_eq!(r.repository(), "my-app");
        assert_eq!(r.tag(), Some("1.2.3"));
        assert!(r.digest().is_none());
        assert!(r.is_valid());
    }

    #[test]
    fn parse_repo_digest() {
        let r = parse("ubuntu@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap();
        assert_eq!(r.registry(), None);
        assert_eq!(r.repository(), "ubuntu");
        assert_eq!(r.tag(), None);
        assert_eq!(
            r.digest(),
            Some("sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert!(r.is_valid());
    }

    #[test]
    fn parse_registry_with_port() {
        let r = parse("registry.example.org:5000/my-app:latest").unwrap();
        assert_eq!(r.registry(), Some("registry.example.org:5000"));
        assert_eq!(r.repository(), "my-app");
        assert_eq!(r.tag(), Some("latest"));
        assert!(r.digest().is_none());
    }

    #[test]
    fn parse_multi_path_repository() {
        let r = parse("docker.io/library/ubuntu:24.04").unwrap();
        assert_eq!(r.registry(), Some("docker.io"));
        assert_eq!(r.repository(), "library/ubuntu");
        assert_eq!(r.tag(), Some("24.04"));
    }

    #[test]
    fn parse_empty_returns_err() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
    }

    #[test]
    fn parse_missing_tag_or_digest_is_parseable_but_invalid() {
        // A bare repo name parses but is_valid returns false.
        let r = parse("my-app").unwrap();
        assert_eq!(r.repository(), "my-app");
        assert!(!r.is_valid());
    }

    #[test]
    fn parse_digest_without_algorithm_prefix_returns_err() {
        let err = parse("ubuntu@abc123").unwrap_err();
        assert!(err.contains("algorithm"), "expected algorithm error, got: {err}");
    }

    #[test]
    fn parse_empty_digest_returns_err() {
        let err = parse("ubuntu@").unwrap_err();
        assert!(err.contains("empty digest"), "expected empty digest error, got: {err}");
    }

    // --- Display / to_string ----------------------------------------------

    #[test]
    fn to_string_roundtrip_repo_tag() {
        let s = "registry.example.org/my-app:1.2.3";
        let r = parse(s).unwrap();
        assert_eq!(r.to_string(), s);
    }

    #[test]
    fn to_string_roundtrip_repo_digest() {
        let s = "ubuntu@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let r = parse(s).unwrap();
        assert_eq!(r.to_string(), s);
    }

    #[test]
    fn to_string_roundtrip_registry_with_port() {
        let s = "registry.example.org:5000/my-app:latest";
        let r = parse(s).unwrap();
        assert_eq!(r.to_string(), s);
    }

    // --- with_tag / with_digest -------------------------------------------

    #[test]
    fn with_tag_no_registry() {
        assert_eq!(with_tag(None, "my-app", "1.0.0"), "my-app:1.0.0");
    }

    #[test]
    fn with_tag_with_registry() {
        assert_eq!(
            with_tag(Some("registry.example.org"), "my-app", "1.0.0"),
            "registry.example.org/my-app:1.0.0"
        );
    }

    #[test]
    fn with_digest_no_registry() {
        assert_eq!(with_digest(None, "my-app", "sha256:abc"), "my-app@sha256:abc");
    }

    #[test]
    fn with_digest_with_registry() {
        assert_eq!(
            with_digest(Some("docker.io"), "library/ubuntu", "sha256:abc"),
            "docker.io/library/ubuntu@sha256:abc"
        );
    }

    // --- is_valid ----------------------------------------------------------

    #[test]
    fn is_valid_accepts_tagged_ref() {
        assert!(is_valid("ubuntu:22.04"));
    }

    #[test]
    fn is_valid_accepts_digest_ref() {
        assert!(is_valid("ubuntu@sha256:abc"));
    }

    #[test]
    fn is_valid_rejects_bare_repo() {
        assert!(!is_valid("ubuntu"));
    }

    #[test]
    fn is_valid_rejects_empty() {
        assert!(!is_valid(""));
    }
}
