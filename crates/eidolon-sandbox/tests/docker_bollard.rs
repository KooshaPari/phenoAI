//! Docker / bollard feature-gated tests (A+ T2 Phase C).
//!
//! Default `cargo test --locked` stays green without a Docker daemon.
//! Live start/stop/exec requires `--features sandbox-docker` and
//! `DOCKER_INTEGRATION=1`.

use eidolon_core::error::PhenoError;
use eidolon_sandbox::codes;
use eidolon_sandbox::{docker_probe, StubDockerOrchestrator};
use eidolon_sandbox::DockerOrchestrator;

fn assert_docker_stub(err: PhenoError) {
    assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_DOCKER_STUB));
    assert_eq!(err.status_code(), 501);
}

#[tokio::test]
async fn stub_always_fail_loud_without_feature_path() {
    let stub = StubDockerOrchestrator::new();
    assert!(!stub.docker_ready());
    assert_docker_stub(
        stub.start_container("alpine:3", Default::default())
            .await
            .unwrap_err(),
    );
}

#[test]
fn probe_docker_consistency() {
    let ready = docker_probe::docker_ready();
    if docker_probe::docker_cli_ready() {
        assert!(ready, "CLI-ready implies docker_ready");
        assert!(docker_probe::resolve_docker_cli().is_some());
    }
}

#[cfg(feature = "sandbox-docker")]
mod with_feature {
    use super::*;
    use eidolon_core::traits::SandboxAutomator;
    use eidolon_sandbox::{BollardDockerOrchestrator, DockerSandboxClient};

    #[tokio::test]
    async fn try_new_matches_daemon() {
        match BollardDockerOrchestrator::try_new().await {
            Ok(orch) => {
                assert!(orch.docker_ready());
            }
            Err(err) => {
                assert_docker_stub(err);
            }
        }
    }

    #[tokio::test]
    async fn sandbox_client_try_new_matches_daemon() {
        match DockerSandboxClient::try_new("eidolon-t2-probe").await {
            Ok(client) => {
                assert!(client.docker_ready());
                let meta = client.get_metadata().await.expect("metadata");
                assert_eq!(meta.id, "eidolon-t2-probe");
                assert_eq!(meta.image, DockerSandboxClient::DEFAULT_IMAGE);
            }
            Err(err) => {
                assert_docker_stub(err);
            }
        }
    }

    #[tokio::test]
    async fn live_lifecycle_when_integration_env() {
        if std::env::var("DOCKER_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let client = DockerSandboxClient::try_new("eidolon-t2-live")
            .await
            .expect("DOCKER_INTEGRATION=1 requires a reachable Docker daemon");

        client.start().await.expect("start");
        let out = client.exec("echo eidolon-ok").await.expect("exec");
        assert!(
            out.contains("eidolon-ok"),
            "unexpected exec output: {out:?}"
        );
        let usage = client.resource_usage().await.expect("stats");
        assert!(usage.memory_mb < u32::MAX);
        client.stop().await.expect("stop");
    }
}
