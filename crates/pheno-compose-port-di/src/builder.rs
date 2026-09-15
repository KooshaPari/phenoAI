// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-port-di::builder` — `ContainerBuilder`.

use phenocompose_port_composer::{Composer, NoopComposer};
use phenocompose_port_publisher::{NoopPublisher, Publisher};
use phenocompose_port_runtime::{NoopRuntime, Runtime};
use phenocompose_port_secret::{InMemorySecretStore, SecretStore};

use crate::Container;

/// Builder for [`Container`]. Each `with_*` method replaces the
/// adapter for the corresponding port; any port left unset
/// falls back to the in-memory / no-op default.
pub struct ContainerBuilder {
    /// Optional override for the [`Composer`] port.
    composer: Option<Box<dyn Composer>>,
    /// Optional override for the [`Publisher`] port.
    publisher: Option<Box<dyn Publisher>>,
    /// Optional override for the [`Runtime`] port.
    runtime: Option<Box<dyn Runtime>>,
    /// Optional override for the [`SecretStore`] port.
    secrets: Option<Box<dyn SecretStore>>,
}

// Manual `Debug` mirrors [`Container`]'s: print the names of
// the currently-overridden adapters (or `None`).
impl core::fmt::Debug for ContainerBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ContainerBuilder")
            .field(
                "composer",
                &self.composer.as_ref().map(|c| c.name().to_owned()),
            )
            .field(
                "publisher",
                &self.publisher.as_ref().map(|p| p.name().to_owned()),
            )
            .field(
                "runtime",
                &self.runtime.as_ref().map(|r| r.name().to_owned()),
            )
            .field(
                "secrets",
                &self.secrets.as_ref().map(|s| s.name().to_owned()),
            )
            .finish()
    }
}

impl ContainerBuilder {
    /// Construct a fresh builder with no overrides. Every
    /// port will fall back to its in-memory / no-op default
    /// at [`ContainerBuilder::build`] time.
    pub fn new() -> Self {
        Self {
            composer: None,
            publisher: None,
            runtime: None,
            secrets: None,
        }
    }

    /// Replace the [`Composer`] adapter. The argument is
    /// boxed and stored as `Box<dyn Composer>`.
    pub fn with_composer(mut self, composer: impl Composer + 'static) -> Self {
        self.composer = Some(Box::new(composer));
        self
    }

    /// Replace the [`Publisher`] adapter.
    pub fn with_publisher(mut self, publisher: impl Publisher + 'static) -> Self {
        self.publisher = Some(Box::new(publisher));
        self
    }

    /// Replace the [`Runtime`] adapter.
    pub fn with_runtime(mut self, runtime: impl Runtime + 'static) -> Self {
        self.runtime = Some(Box::new(runtime));
        self
    }

    /// Replace the [`SecretStore`] adapter.
    pub fn with_secrets(mut self, secrets: impl SecretStore + 'static) -> Self {
        self.secrets = Some(Box::new(secrets));
        self
    }

    /// Finalize the builder, falling back to the in-memory
    /// defaults for any port that wasn't overridden.
    pub fn build(self) -> Container {
        Container {
            composer: self.composer.unwrap_or_else(|| Box::new(NoopComposer)),
            publisher: self.publisher.unwrap_or_else(|| Box::new(NoopPublisher)),
            runtime: self.runtime.unwrap_or_else(|| Box::new(NoopRuntime::new())),
            secrets: self.secrets.unwrap_or_else(|| Box::new(InMemorySecretStore::new())),
        }
    }
}

impl Default for ContainerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
