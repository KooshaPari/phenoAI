// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-apple-container-adapter`
//!
//! Apple container CLI-backed [`Runtime`](phenocompose_port_runtime::Runtime)
//! adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use phenocompose_port_runtime::{Runtime, RuntimeError};
use phenocompose_port_types::{ContainerId, ContainerStatus, ImageRef};

/// Runtime adapter for Apple's `/usr/local/bin/container` CLI.
#[derive(Debug, Default)]
pub struct AppleContainerRuntime;

impl AppleContainerRuntime {
    /// Construct a new Apple container runtime adapter.
    pub fn new() -> Self {
        Self
    }
}

impl Runtime for AppleContainerRuntime {
    fn spawn(&self, image: &ImageRef) -> Result<ContainerId, RuntimeError> {
        if image.as_ref().is_empty() {
            return Err(RuntimeError::validation("image reference is empty"));
        }

        spawn_container(image)
    }

    fn stop(&self, id: &ContainerId) -> Result<(), RuntimeError> {
        stop_container(id)
    }

    fn status(&self, id: &ContainerId) -> Result<ContainerStatus, RuntimeError> {
        container_status(id)
    }

    fn name(&self) -> &str {
        "apple-container"
    }
}

#[cfg(target_os = "macos")]
fn spawn_container(image: &ImageRef) -> Result<ContainerId, RuntimeError> {
    let output = std::process::Command::new("/usr/local/bin/container")
        .args(["run", "-d", image.as_ref()])
        .output()
        .map_err(|e| RuntimeError::backend(format!("failed to run container CLI: {e}")))?;

    if !output.status.success() {
        return Err(RuntimeError::backend(command_error("container run", &output)));
    }

    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if id.is_empty() {
        return Err(RuntimeError::backend("container run returned an empty container id"));
    }

    Ok(ContainerId::new(id))
}

#[cfg(not(target_os = "macos"))]
fn spawn_container(_image: &ImageRef) -> Result<ContainerId, RuntimeError> {
    Err(RuntimeError::backend(
        "apple-container runtime is only available on macOS",
    ))
}

#[cfg(target_os = "macos")]
fn stop_container(id: &ContainerId) -> Result<(), RuntimeError> {
    let output = std::process::Command::new("/usr/local/bin/container")
        .args(["stop", id.as_ref()])
        .output()
        .map_err(|e| RuntimeError::backend(format!("failed to stop container: {e}")))?;

    if output.status.success() {
        Ok(())
    } else if output_mentions_not_found(&output) {
        Err(RuntimeError::not_found(format!("no container with id {}", id.as_ref())))
    } else {
        Err(RuntimeError::backend(command_error("container stop", &output)))
    }
}

#[cfg(not(target_os = "macos"))]
fn stop_container(_id: &ContainerId) -> Result<(), RuntimeError> {
    Err(RuntimeError::backend(
        "apple-container runtime is only available on macOS",
    ))
}

#[cfg(target_os = "macos")]
fn container_status(id: &ContainerId) -> Result<ContainerStatus, RuntimeError> {
    let inspect = std::process::Command::new("/usr/local/bin/container")
        .args(["inspect", id.as_ref()])
        .output()
        .map_err(|e| RuntimeError::backend(format!("failed to inspect container: {e}")))?;

    if inspect.status.success() {
        return Ok(parse_status(&String::from_utf8_lossy(&inspect.stdout)));
    }

    if output_mentions_not_found(&inspect) {
        return Ok(ContainerStatus::NotFound);
    }

    let list = std::process::Command::new("/usr/local/bin/container")
        .args(["ls", "-a"])
        .output()
        .map_err(|e| RuntimeError::backend(format!("failed to list containers: {e}")))?;

    if !list.status.success() {
        return Err(RuntimeError::backend(command_error("container inspect", &inspect)));
    }

    let listing = String::from_utf8_lossy(&list.stdout);
    if !listing.contains(id.as_ref()) {
        return Ok(ContainerStatus::NotFound);
    }

    Ok(parse_status(&listing))
}

#[cfg(not(target_os = "macos"))]
fn container_status(_id: &ContainerId) -> Result<ContainerStatus, RuntimeError> {
    Err(RuntimeError::backend(
        "apple-container runtime is only available on macOS",
    ))
}

#[cfg(target_os = "macos")]
fn parse_status(output: &str) -> ContainerStatus {
    let output = output.to_ascii_lowercase();
    if output.contains("paused") {
        ContainerStatus::Paused
    } else if output.contains("running") {
        ContainerStatus::Running
    } else if output.contains("exited")
        || output.contains("stopped")
        || output.contains("created")
        || output.contains("dead")
    {
        ContainerStatus::Exited
    } else {
        ContainerStatus::Running
    }
}

#[cfg(target_os = "macos")]
fn output_mentions_not_found(output: &std::process::Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    stderr.contains("not found") || stderr.contains("no such") || stderr.contains("does not exist")
}

#[cfg(target_os = "macos")]
fn command_error(command: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        format!("{command} failed with status {}", output.status)
    } else {
        format!("{command} failed with status {}: {stderr}", output.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phenocompose_port_runtime::{Runtime, RuntimeError};
    use phenocompose_port_types::ImageRef;

    #[test]
    fn apple_container_runtime_name_is_stable() {
        let r = AppleContainerRuntime::new();
        assert_eq!(r.name(), "apple-container");
    }

    #[test]
    fn apple_container_runtime_rejects_empty_image() {
        let r = AppleContainerRuntime::new();
        let err = r.spawn(&ImageRef::new("")).unwrap_err();
        assert!(matches!(err, RuntimeError::Validation(_)));
    }

    #[test]
    fn apple_container_runtime_is_object_safe() {
        fn _takes_dyn(_r: &dyn Runtime) {}
        let r = AppleContainerRuntime::new();
        _takes_dyn(&r);
        let _boxed: Box<dyn Runtime> = Box::new(r);
    }
}
