// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-types`
//!
//! Shared value types that flow across the PhenoCompose port
//! traits (Composer, Publisher, Runtime, SecretStore). Defined
//! as a standalone crate so the port-trait crates can depend on
//! a single canonical type vocabulary without pulling in each
//! other's implementation code or test fixtures.
//!
//! Type inventory:
//!
//! | Type               | Role                                                 |
//! |--------------------|------------------------------------------------------|
//! | [`Manifest`]       | Input to the [`Composer`](crate::Composer) port      |
//! | [`ComposedArtifact`] | Output of `Composer`; input to [`Publisher`](crate::Publisher) |
//! | [`PublishTarget`]  | Where a [`ComposedArtifact`] is sent                 |
//! | [`PublishReceipt`] | Proof of a successful publish                        |
//! | [`ImageRef`]       | Reference to a container image; input to [`Runtime`](crate::Runtime) |
//! | [`ContainerId`]    | Opaque handle returned by `Runtime::spawn`           |
//! | [`ContainerStatus`] | State reported by `Runtime::status`                  |
//! | [`SecretRef`]      | Strongly-typed identifier for a [`Secret`]            |
//! | [`Secret`]         | A versioned, named value stored by a `SecretStore`  |
//!
//! All types in this crate are `Send + Sync` so they can be moved
//! across worker threads and stored in `Box<dyn Trait>` adapters
//! that downstream pheno-* services compose into their dependency
//! graph.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

// ---------------------------------------------------------------------------
// OCI helpers — canonical home is `phenotype-types`
// ---------------------------------------------------------------------------
pub mod compose;
pub mod error;
/// OCI (Open Container Initiative) image reference helpers.
///
/// Parsing, validation, and construction utilities for OCI image
/// references. See the [`oci` module documentation](oci) for details.
///
/// The **canonical** home for these helpers in the Phenotype
/// ecosystem is the **`phenotype-types`** crate
/// (<https://github.com/kooshapari/phenotype-types>). Consumers
/// SHOULD prefer that crate over this local module when it is
/// available.
pub mod oci;
pub mod runtime;
pub mod secret;

// ---------------------------------------------------------------------------
// Re-exports — the public API stays at the crate root so downstream
// `use phenocompose_port_types::Manifest;` keeps working unchanged.
// ---------------------------------------------------------------------------
pub use compose::{ComposedArtifact, Manifest, PublishReceipt, PublishTarget};
pub use error::PortError;
pub use runtime::{ContainerId, ContainerStatus, ImageRef};
pub use secret::{Secret, SecretRef};

#[cfg(test)]
mod tests {
    #[test]
    fn manifest_new_sets_name_and_leaves_others_empty() {
        let m = Manifest::new("phenocommand-web");
        assert_eq!(m.name, "phenocommand-web");
        assert!(m.artifact_name.is_none());
        assert!(m.tags.is_empty());
    }

    #[test]
    fn manifest_builder_set_artifact_name_and_tags() {
        let m = Manifest::new("phenocommand-web")
            .with_artifact_name("phenocommand-web:0.1.0")
            .with_tag("channel", "stable")
            .with_tag("version", "0.1.0");
        assert_eq!(m.artifact_name.as_deref(), Some("phenocommand-web:0.1.0"));
        assert_eq!(m.tags.len(), 2);
        assert_eq!(m.tags[0], ("channel".to_string(), "stable".to_string()));
        assert_eq!(m.tags[1], ("version".to_string(), "0.1.0".to_string()));
    }

    #[test]
    fn composed_artifact_tag_lookup() {
        let a = ComposedArtifact::new("phenocommand-web:0.1.0", ImageRef::new("phenocommand-web:0.1.0"))
            .with_tag("content-digest", "sha256:abc");
        assert_eq!(a.tag("content-digest"), Some("sha256:abc"));
        assert_eq!(a.tag("missing"), None);
    }

    #[test]
    fn image_ref_with_tag_joins_correctly() {
        let r = ImageRef::with_tag("phenocommand-web", "0.1.0");
        assert_eq!(r.reference, "phenocommand-web:0.1.0");
        assert_eq!(r.as_ref(), "phenocommand-web:0.1.0");
        assert_eq!(format!("{r}"), "phenocommand-web:0.1.0");
    }

    #[test]
    fn image_ref_from_str_and_string() {
        let from_str: ImageRef = "phenocommand-web:0.1.0".into();
        let from_string: ImageRef = String::from("phenocommand-web:0.2.0").into();
        assert_eq!(from_str.reference, "phenocommand-web:0.1.0");
        assert_eq!(from_string.reference, "phenocommand-web:0.2.0");
    }

    #[test]
    fn container_id_display_and_as_ref() {
        let id = ContainerId::new("abc123");
        assert_eq!(id.as_ref(), "abc123");
        assert_eq!(format!("{id}"), "abc123");
    }

    #[test]
    fn container_status_is_active_and_display() {
        assert!(ContainerStatus::Running.is_active());
        assert!(ContainerStatus::Paused.is_active());
        assert!(!ContainerStatus::Exited.is_active());
        assert!(!ContainerStatus::NotFound.is_active());

        assert_eq!(format!("{}", ContainerStatus::Running), "running");
        assert_eq!(format!("{}", ContainerStatus::Exited), "exited");
        assert_eq!(format!("{}", ContainerStatus::Paused), "paused");
        assert_eq!(format!("{}", ContainerStatus::NotFound), "not_found");
    }

    #[test]
    fn publish_target_new() {
        let t = PublishTarget::new("docker-registry", "registry.phenotype/phenocommand/web:0.1.0");
        assert_eq!(t.kind, "docker-registry");
        assert_eq!(t.locator, "registry.phenotype/phenocommand/web:0.1.0");
    }

    #[test]
    fn publish_receipt_new() {
        let t = PublishTarget::new("file", "/var/lib/phenocompose/x.tar");
        let r = PublishReceipt::new("phenocommand-web:0.1.0", t.clone(), "/var/lib/phenocompose/x.tar");
        assert_eq!(r.artifact_id, "phenocommand-web:0.1.0");
        assert_eq!(r.target, t);
        assert_eq!(r.published_at, "/var/lib/phenocompose/x.tar");
    }

    #[test]
    fn port_error_display_mentions_kind_and_context() {
        let e = PortError::Validation("empty name".to_string());
        let s = format!("{e}");
        assert!(s.contains("validation"));
        assert!(s.contains("empty name"));
    }

    #[test]
    fn secret_ref_new_uses_empty_namespace() {
        let r = SecretRef::new("db-password");
        assert_eq!(r.name, "db-password");
        assert_eq!(r.namespace, "");
        assert_eq!(r.locator(), "db-password");
        assert_eq!(r.as_ref(), "db-password");
        assert_eq!(format!("{r}"), "db-password");
    }

    #[test]
    fn secret_ref_namespaced_renders_as_namespace_slash_name() {
        let r = SecretRef::namespaced("phenotype", "tls-cert");
        assert_eq!(r.namespace, "phenotype");
        assert_eq!(r.name, "tls-cert");
        assert_eq!(r.locator(), "phenotype/tls-cert");
        assert_eq!(format!("{r}"), "phenotype/tls-cert");
    }

    #[test]
    fn secret_new_defaults_to_version_one() {
        let r = SecretRef::namespaced("phenotype", "api-key");
        let s = Secret::new(r.clone(), "s3cr3t");
        assert_eq!(s.r#ref, r);
        assert_eq!(s.value, "s3cr3t");
        assert_eq!(s.version, 1);
    }

    #[test]
    fn secret_at_version_overrides_counter() {
        let r = SecretRef::new("db-password");
        let s = Secret::new(r, "hunter2").at_version(7);
        assert_eq!(s.version, 7);
    }
}
