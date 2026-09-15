// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-di`
//!
//! The PhenoCompose dependency-injection container. Wires the
//! four port-trait adapters (Composer, Publisher, Runtime,
//! SecretStore) into a single [`Container`] value that
//! downstream pheno-* services can hold in one place.
//!
//! # Why a container?
//!
//! The port traits are object-safe (`Box<dyn Trait>`), but
//! spreading four `Box<dyn ...>` fields across every service
//! that wants to use them is boilerplate. [`Container`] packages
//! the four port adapters behind accessor methods and provides
//! a [`ContainerBuilder`] for swapping in concrete backends
//! (FileSecretStore, a real Docker-backed [`Runtime`], ...)
//! without touching call sites.
//!
//! # Defaults
//!
//! [`Container::default`] wires the in-memory / no-op adapters
//! from each port crate:
//!
//! - [`NoopComposer`](phenocompose_port_composer::NoopComposer)
//! - [`NoopPublisher`](phenocompose_port_publisher::NoopPublisher)
//! - [`NoopRuntime`](phenocompose_port_runtime::NoopRuntime)
//! - [`InMemorySecretStore`](phenocompose_port_secret::InMemorySecretStore)
//!
//! These defaults are useful in tests, dry-run modes, and the
//! DI container's own unit tests; production callers should
//! use [`ContainerBuilder`] to swap in concrete backends.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use phenocompose_port_composer::{Composer, NoopComposer};
use crate::builder::ContainerBuilder;
use phenocompose_port_publisher::{NoopPublisher, Publisher};
use phenocompose_port_runtime::{NoopRuntime, Runtime};
use phenocompose_port_secret::{InMemorySecretStore, SecretStore};

pub mod builder;

/// The PhenoCompose DI container — holds one adapter per port
/// trait and exposes them through borrowed accessors so call
/// sites don't need to know which concrete adapter is wired.
pub struct Container {
    /// The [`Composer`] adapter (e.g. cargo, docker buildx,
    /// noop).
    composer: Box<dyn Composer>,
    /// The [`Publisher`] adapter (e.g. docker registry, file,
    /// noop).
    publisher: Box<dyn Publisher>,
    /// The [`Runtime`] adapter (e.g. docker, podman, noop).
    runtime: Box<dyn Runtime>,
    /// The [`SecretStore`] adapter (e.g. in-memory, file,
    /// vault, noop).
    secrets: Box<dyn SecretStore>,
}

// Manual `Debug` because the wrapped trait objects don't
// implement `Debug` themselves; we only need the
// adapter-name string for diagnostics.
impl core::fmt::Debug for Container {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Container")
            .field("composer", &self.composer.name())
            .field("publisher", &self.publisher.name())
            .field("runtime", &self.runtime.name())
            .field("secrets", &self.secrets.name())
            .finish()
    }
}

impl Container {
    /// Build a [`Container`] pre-populated with the in-memory
    /// / no-op adapters from each port crate. See the
    /// [crate-level docs](self) for the exact list.
    pub fn default_in_memory() -> Self {
        ContainerBuilder::new().build()
    }

    /// Borrow the wired [`Composer`] adapter.
    pub fn composer(&self) -> &dyn Composer {
        &*self.composer
    }

    /// Borrow the wired [`Publisher`] adapter.
    pub fn publisher(&self) -> &dyn Publisher {
        &*self.publisher
    }

    /// Borrow the wired [`Runtime`] adapter.
    pub fn runtime(&self) -> &dyn Runtime {
        &*self.runtime
    }

    /// Borrow the wired [`SecretStore`] adapter.
    pub fn secrets(&self) -> &dyn SecretStore {
        &*self.secrets
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::default_in_memory()
    }
}


mod tests {
    use super::*;
    use super::*;
    use phenocompose_port_composer::{ComposeError, CountingComposer, NoopComposer};
    use phenocompose_port_publisher::{PublishError, RecordingPublisher};
    use phenocompose_port_runtime::{NoopRuntime, RuntimeError};
    use phenocompose_port_secret::{InMemorySecretStore, NoopSecretStore, SecretStoreError};
    use phenocompose_port_types::{
        ComposedArtifact, ContainerId, ContainerStatus, ImageRef, Manifest, PublishReceipt,
        PublishTarget, Secret, SecretRef,
    };

    #[test]
    fn default_container_uses_in_memory_adapters() {
        let c = Container::default();
        assert_eq!(c.composer().name(), "noop");
        assert_eq!(c.publisher().name(), "noop");
        assert_eq!(c.runtime().name(), "noop");
        assert_eq!(c.secrets().name(), "memory");
    }

    #[test]
    fn default_in_memory_constructor_matches_default_trait() {
        let from_default = Container::default();
        let from_ctor = Container::default_in_memory();
        // NoopComposer / NoopPublisher are unit structs, so
        // we can only compare by name; for the in-memory
        // secret store, the empty list is identical.
        assert_eq!(from_default.composer().name(), from_ctor.composer().name());
        assert_eq!(from_default.publisher().name(), from_ctor.publisher().name());
        assert_eq!(from_default.runtime().name(), from_ctor.runtime().name());
        assert_eq!(from_default.secrets().name(), from_ctor.secrets().name());
        assert!(from_ctor.secrets().list("default").unwrap().is_empty());
    }

    #[test]
    fn builder_with_composer_overrides_only_composer() {
        let c = ContainerBuilder::new()
            .with_composer(CountingComposer::new())
            .build();
        assert_eq!(c.composer().name(), "counting");
        assert_eq!(c.publisher().name(), "noop");
        assert_eq!(c.runtime().name(), "noop");
        assert_eq!(c.secrets().name(), "memory");
    }

    #[test]
    fn builder_with_publisher_overrides_only_publisher() {
        let c = ContainerBuilder::new()
            .with_publisher(RecordingPublisher::new())
            .build();
        assert_eq!(c.composer().name(), "noop");
        assert_eq!(c.publisher().name(), "recording");
        assert_eq!(c.runtime().name(), "noop");
        assert_eq!(c.secrets().name(), "memory");
    }

    #[test]
    fn builder_with_runtime_overrides_only_runtime() {
        let c = ContainerBuilder::new()
            .with_runtime(NoopRuntime::new())
            .build();
        assert_eq!(c.composer().name(), "noop");
        assert_eq!(c.publisher().name(), "noop");
        assert_eq!(c.runtime().name(), "noop");
        assert_eq!(c.secrets().name(), "memory");
    }

    #[test]
    fn builder_with_secrets_overrides_only_secrets() {
        let c = ContainerBuilder::new()
            .with_secrets(NoopSecretStore)
            .build();
        assert_eq!(c.composer().name(), "noop");
        assert_eq!(c.publisher().name(), "noop");
        assert_eq!(c.runtime().name(), "noop");
        assert_eq!(c.secrets().name(), "noop");
    }

    #[test]
    fn builder_with_all_overrides_wires_each_port() {
        let c = ContainerBuilder::new()
            .with_composer(CountingComposer::new())
            .with_publisher(RecordingPublisher::new())
            .with_runtime(NoopRuntime::new())
            .with_secrets(InMemorySecretStore::new())
            .build();
        assert_eq!(c.composer().name(), "counting");
        assert_eq!(c.publisher().name(), "recording");
        assert_eq!(c.runtime().name(), "noop");
        assert_eq!(c.secrets().name(), "memory");
    }

    #[test]
    fn default_builder_matches_default_container() {
        let from_default = Container::default();
        let from_builder = ContainerBuilder::new().build();
        assert_eq!(from_default.composer().name(), from_builder.composer().name());
        assert_eq!(from_default.publisher().name(), from_builder.publisher().name());
        assert_eq!(from_default.runtime().name(), from_builder.runtime().name());
        assert_eq!(from_default.secrets().name(), from_builder.secrets().name());
    }

    #[test]
    fn container_can_drive_a_full_compose_publish_spawn_secret_workflow() {
        // Build a container with the recording / counting
        // adapters and exercise every port through the
        // container's accessors. This is the integration test
        // for "all four ports are wired into one DI object".
        let secrets = InMemorySecretStore::new();
        let c = ContainerBuilder::new()
            .with_composer(CountingComposer::new())
            .with_publisher(RecordingPublisher::new())
            .with_runtime(NoopRuntime::new())
            .with_secrets(secrets)
            .build();

        // 1. Compose a manifest into an artifact.
        let m = Manifest::new("phenocommand-web");
        let artifact: ComposedArtifact = c.composer().compose(&m).expect("compose ok");
        assert_eq!(artifact.id, "phenocommand-web:counted");

        // 2. Publish the artifact to a local file target.
        let target = PublishTarget::new("file", "/tmp/phenocommand-web:0.1.0.tar");
        let receipt: PublishReceipt = c
            .publisher()
            .publish(&artifact, &target)
            .expect("publish ok");
        assert_eq!(receipt.artifact_id, artifact.id);

        // 3. Spawn a container from the artifact's image.
        let id: ContainerId = c
            .runtime()
            .spawn(&artifact.image)
            .expect("spawn ok");
        let status: ContainerStatus = c.runtime().status(&id).expect("status ok");
        assert_eq!(status, ContainerStatus::Running);

        // 4. Store a secret referencing the artifact.
        let secret_ref = SecretRef::new("db-password");
        let stored: Secret = c
            .secrets()
            .put(&Secret::new(secret_ref.clone(), "hunter2"))
            .expect("put ok");
        assert_eq!(stored.version, 1);
        let got: Secret = c.secrets().get(&secret_ref).expect("get ok");
        assert_eq!(got.value, "hunter2");
    }

    #[test]
    fn container_propagates_errors_through_each_port() {
        // Sanity-check that the container is transparent:
        // every adapter's error type bubbles up to the
        // accessor without being swallowed or re-wrapped.
        let c = Container::default();

        let compose_err = c
            .composer()
            .compose(&Manifest::new(""))
            .expect_err("empty manifest must be rejected");
        assert!(matches!(compose_err, ComposeError::Validation(_)));

        let publish_err = c
            .publisher()
            .publish(
                &ComposedArtifact::new("", ImageRef::new("")),
                &PublishTarget::new("file", "/tmp/x"),
            )
            .expect_err("empty artifact id must be rejected");
        assert!(matches!(publish_err, PublishError::Validation(_)));

        let runtime_err = c
            .runtime()
            .spawn(&ImageRef::new(""))
            .expect_err("empty image ref must be rejected");
        assert!(matches!(runtime_err, RuntimeError::Validation(_)));

        let secret_err = c
            .secrets()
            .get(&SecretRef::new(""))
            .expect_err("empty secret name must be rejected");
        assert!(matches!(secret_err, SecretStoreError::Validation(_)));
    }

    #[test]
    fn container_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Container>();
        assert_send_sync::<ContainerBuilder>();
    }

    #[test]
    fn builder_default_trait_works() {
        let _b: ContainerBuilder = ContainerBuilder::default();
        let c = ContainerBuilder::default().build();
        assert_eq!(c.composer().name(), NoopComposer.name());
    }
}
