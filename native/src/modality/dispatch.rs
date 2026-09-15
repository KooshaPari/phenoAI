//! Per-port modality dispatch — wires selected modality into App ports.
//!
//! ADR-006 M2+: when `--modality sandbox` is selected, process lifecycle
//! routes through [`WireSandboxAdapter`] / [`SandboxDriver`]. Capture,
//! input, and window ports tunnel via stdio JSON-RPC to `playcua-bridge`
//! ([`SandboxBridgePorts`]) — never native host adapters (that would leak
//! host OS calls into the sandbox path). Missing bridge → fail loud.
//!
//! Native modality keeps the existing platform adapters unchanged.

use std::sync::Arc;

use async_trait::async_trait;

use crate::adapters::analysis_adapter::NativeAnalysisAdapter;
use crate::adapters::process_adapter::NativeProcessAdapter;
use crate::adapters::sandbox::WireSandboxAdapter;
use crate::adapters::sandbox_bridge::SandboxBridgePorts;
use crate::domain::capture::{CaptureError, Frame};
use crate::domain::input::{InputError, Key, KeyAction, MouseEvent};
use crate::domain::process::{ProcessError, ProcessHandle, ProcessStatus};
use crate::domain::sandbox::SandboxSpec;
use crate::domain::window::{WindowError, WindowFilter, WindowInfo};
use crate::modality::registry::SelectedModality;
use crate::modality::ModalityKind;
use crate::ports::sandbox::Sandbox;
use crate::ports::{AnalysisPort, CapturePort, InputPort, ProcessPort, WindowPort};

/// Bundle of port trait objects selected for a modality.
pub struct PortBundle {
    pub capture: Arc<dyn CapturePort>,
    pub input: Arc<dyn InputPort>,
    pub windows: Arc<dyn WindowPort>,
    pub process: Arc<dyn ProcessPort>,
    pub analysis: Arc<dyn AnalysisPort>,
    /// Shared sandbox port when modality is Sandbox (also used by process).
    pub sandbox: Option<Arc<dyn Sandbox>>,
}

/// Build the port bundle for `selected`. Native uses platform adapters;
/// Sandbox uses driver-backed process dispatch + JSON-RPC bridge I/O ports.
pub fn build_ports(
    selected: &SelectedModality,
    native_capture: Arc<dyn CapturePort>,
    native_input: Arc<dyn InputPort>,
    native_windows: Arc<dyn WindowPort>,
) -> PortBundle {
    match selected.kind {
        ModalityKind::Native => PortBundle {
            capture: native_capture,
            input: native_input,
            windows: native_windows,
            process: Arc::new(NativeProcessAdapter::new()),
            analysis: Arc::new(NativeAnalysisAdapter::new()),
            sandbox: None,
        },
        ModalityKind::Sandbox => build_sandbox_ports(selected),
        other => {
            // Nvms / Wsl / Container drivers exist but are not yet wired into
            // per-port dispatch. Fail loud on process.launch rather than
            // silently executing on the host (would violate modality isolation).
            tracing::warn!(
                kind = %other,
                "modality selected but per-port dispatch not wired; \
                 process/capture/input fail-loud until M3–M5 dispatch lands"
            );
            let detail = format!(
                "modality `{other}` has a driver API but no per-port dispatch yet \
                 (ADR-006); use --modality native or sandbox"
            );
            PortBundle {
                capture: Arc::new(FailLoudCapture {
                    reason: detail.clone(),
                }),
                input: Arc::new(FailLoudInput {
                    reason: detail.clone(),
                }),
                windows: Arc::new(FailLoudWindow {
                    reason: detail.clone(),
                }),
                process: Arc::new(FailLoudProcess { reason: detail }),
                analysis: Arc::new(NativeAnalysisAdapter::new()),
                sandbox: None,
            }
        }
    }
}

fn build_sandbox_ports(selected: &SelectedModality) -> PortBundle {
    if !selected.available {
        let reason = format!(
            "sandbox modality selected but unavailable ({}); install \
             firejail/sandbox-exec/runsc or set PLAYCUA_SANDBOX_BACKEND=direct",
            selected.detail
        );
        tracing::error!(%reason, "refusing native fallback for sandbox modality");
        return PortBundle {
            capture: Arc::new(FailLoudCapture {
                reason: reason.clone(),
            }),
            input: Arc::new(FailLoudInput {
                reason: reason.clone(),
            }),
            windows: Arc::new(FailLoudWindow {
                reason: reason.clone(),
            }),
            process: Arc::new(FailLoudProcess { reason }),
            analysis: Arc::new(NativeAnalysisAdapter::new()),
            sandbox: None,
        };
    }

    let adapter = WireSandboxAdapter::new();
    let bridge_slot = adapter.bridge_slot();
    let sandbox: Arc<dyn Sandbox> = Arc::new(adapter);
    let process: Arc<dyn ProcessPort> = Arc::new(SandboxProcessAdapter {
        sandbox: Arc::clone(&sandbox),
    });
    // I/O ports share the SandboxDriver-spawned bridge slot (filled on
    // guest spawn or on first I/O via spawn_bridge). Fail-loud if missing
    // — never falls back to native host adapters.
    let bridge = SandboxBridgePorts::from_shared_slot(bridge_slot);
    tracing::info!(
        detail = %selected.detail,
        "sandbox I/O ports wired to SandboxDriver-managed playcua-bridge"
    );
    PortBundle {
        capture: bridge.capture(),
        input: bridge.input(),
        windows: bridge.windows(),
        process,
        analysis: Arc::new(NativeAnalysisAdapter::new()),
        sandbox: Some(sandbox),
    }
}

/// ProcessPort that forwards launch/kill/status through the Sandbox port.
struct SandboxProcessAdapter {
    sandbox: Arc<dyn Sandbox>,
}

#[async_trait]
impl ProcessPort for SandboxProcessAdapter {
    async fn launch(&self, handle: ProcessHandle) -> Result<u32, ProcessError> {
        let spec = SandboxSpec {
            command: handle.path.clone(),
            args: handle.args.clone(),
        };
        let h = self
            .sandbox
            .spawn(&spec)
            .await
            .map_err(|e| ProcessError::LaunchFailed(e.to_string()))?;
        h.id.parse::<u32>()
            .map_err(|e| ProcessError::LaunchFailed(format!("sandbox pid parse: {e}")))
    }

    async fn kill(&self, pid: u32) -> Result<(), ProcessError> {
        use crate::domain::sandbox::SandboxHandle;
        self.sandbox
            .kill(&SandboxHandle {
                id: pid.to_string(),
            })
            .await
            .map_err(|e| match e {
                crate::domain::sandbox::SandboxError::NotFound(_) => ProcessError::NotFound(pid),
                other => ProcessError::KillFailed(other.to_string()),
            })
    }

    async fn status(&self, pid: u32) -> Result<ProcessStatus, ProcessError> {
        use crate::domain::sandbox::SandboxHandle;
        let st = self
            .sandbox
            .status(&SandboxHandle {
                id: pid.to_string(),
            })
            .await
            .map_err(|e| match e {
                crate::domain::sandbox::SandboxError::NotFound(_) => ProcessError::NotFound(pid),
                other => ProcessError::StatusFailed(other.to_string()),
            })?;
        Ok(ProcessStatus {
            running: st.running,
            exit_code: st.exit_code,
        })
    }
}

struct FailLoudProcess {
    reason: String,
}

#[async_trait]
impl ProcessPort for FailLoudProcess {
    async fn launch(&self, _: ProcessHandle) -> Result<u32, ProcessError> {
        Err(ProcessError::LaunchFailed(self.reason.clone()))
    }
    async fn kill(&self, pid: u32) -> Result<(), ProcessError> {
        Err(ProcessError::KillFailed(format!(
            "{} (pid={pid})",
            self.reason
        )))
    }
    async fn status(&self, pid: u32) -> Result<ProcessStatus, ProcessError> {
        Err(ProcessError::StatusFailed(format!(
            "{} (pid={pid})",
            self.reason
        )))
    }
}

struct FailLoudCapture {
    reason: String,
}

#[async_trait]
impl CapturePort for FailLoudCapture {
    async fn capture_display(&self, _: u32) -> Result<Frame, CaptureError> {
        Err(CaptureError::CaptureFailed(self.reason.clone()))
    }
    async fn capture_window(&self, _: Option<&str>) -> Result<Frame, CaptureError> {
        Err(CaptureError::CaptureFailed(self.reason.clone()))
    }
}

struct FailLoudInput {
    reason: String,
}

#[async_trait]
impl InputPort for FailLoudInput {
    async fn key_event(&self, _: Key, _: KeyAction) -> Result<(), InputError> {
        Err(InputError::InjectionFailed(self.reason.clone()))
    }
    async fn type_text(&self, _: &str) -> Result<(), InputError> {
        Err(InputError::InjectionFailed(self.reason.clone()))
    }
    async fn mouse_event(&self, _: MouseEvent) -> Result<(), InputError> {
        Err(InputError::InjectionFailed(self.reason.clone()))
    }
}

struct FailLoudWindow {
    reason: String,
}

#[async_trait]
impl WindowPort for FailLoudWindow {
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError> {
        Err(WindowError::Failed(self.reason.clone()))
    }
    async fn find_window(&self, _: WindowFilter) -> Result<Option<WindowInfo>, WindowError> {
        Err(WindowError::Failed(self.reason.clone()))
    }
    async fn focus_window(&self, _: usize) -> Result<(), WindowError> {
        Err(WindowError::Failed(self.reason.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modality::ModalityKind;

    fn selected(kind: ModalityKind, available: bool, detail: &str) -> SelectedModality {
        SelectedModality {
            kind,
            describe: "test",
            detail: detail.to_string(),
            available,
        }
    }

    struct NoopCapture;
    #[async_trait]
    impl CapturePort for NoopCapture {
        async fn capture_display(&self, _: u32) -> Result<Frame, CaptureError> {
            Err(CaptureError::CaptureFailed("noop".into()))
        }
        async fn capture_window(&self, _: Option<&str>) -> Result<Frame, CaptureError> {
            Err(CaptureError::CaptureFailed("noop".into()))
        }
    }
    struct NoopInput;
    #[async_trait]
    impl InputPort for NoopInput {
        async fn key_event(&self, _: Key, _: KeyAction) -> Result<(), InputError> {
            Ok(())
        }
        async fn type_text(&self, _: &str) -> Result<(), InputError> {
            Ok(())
        }
        async fn mouse_event(&self, _: MouseEvent) -> Result<(), InputError> {
            Ok(())
        }
    }
    struct NoopWindow;
    #[async_trait]
    impl WindowPort for NoopWindow {
        async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError> {
            Ok(vec![])
        }
        async fn find_window(&self, _: WindowFilter) -> Result<Option<WindowInfo>, WindowError> {
            Ok(None)
        }
        async fn focus_window(&self, _: usize) -> Result<(), WindowError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn sandbox_unavailable_fails_loud_on_process_launch() {
        let ports = build_ports(
            &selected(ModalityKind::Sandbox, false, "no backend"),
            Arc::new(NoopCapture),
            Arc::new(NoopInput),
            Arc::new(NoopWindow),
        );
        let err = ports
            .process
            .launch(ProcessHandle::new("echo"))
            .await
            .expect_err("must fail loud");
        assert!(err.to_string().contains("unavailable"));
    }

    #[tokio::test]
    async fn sandbox_available_routes_process_through_driver() {
        let _guard = crate::modality::sandbox::SANDBOX_ENV_LOCK.lock().await;
        let _bguard = crate::ipc::bridge_client::BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var("PLAYCUA_SANDBOX_BACKEND").ok();
        let prev_bridge = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
        std::env::set_var("PLAYCUA_SANDBOX_BACKEND", "direct");
        let bin = {
            let fixture = if cfg!(windows) {
                "fake-playcua-bridge.cmd"
            } else {
                "fake-playcua-bridge.sh"
            };
            let mut candidates = vec![];
            if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
                candidates.push(
                    std::path::PathBuf::from(m)
                        .join("tests/fixtures")
                        .join(fixture),
                );
            }
            candidates.push(std::path::PathBuf::from("tests/fixtures").join(fixture));
            candidates
                .into_iter()
                .find(|p| p.is_file())
                .expect("platform fake-playcua-bridge fixture")
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).ok();
        }
        std::env::set_var("PLAYCUA_BRIDGE_BIN", &bin);
        let ports = build_ports(
            &selected(ModalityKind::Sandbox, true, "backend=direct"),
            Arc::new(NoopCapture),
            Arc::new(NoopInput),
            Arc::new(NoopWindow),
        );
        assert!(ports.sandbox.is_some());
        #[cfg(unix)]
        let handle = ProcessHandle::new("sleep").with_args(vec!["30".into()]);
        #[cfg(windows)]
        let handle = ProcessHandle::new("cmd")
            .with_args(vec!["/C".into(), "ping -n 30 127.0.0.1 >NUL".into()]);
        let pid = ports
            .process
            .launch(handle)
            .await
            .expect("sandbox process launch");
        assert!(pid > 0);
        ports.process.kill(pid).await.expect("kill");
        match prev {
            Some(v) => std::env::set_var("PLAYCUA_SANDBOX_BACKEND", v),
            None => std::env::remove_var("PLAYCUA_SANDBOX_BACKEND"),
        }
        match prev_bridge {
            Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
            None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
        }
    }

    #[tokio::test]
    async fn sandbox_capture_does_not_silently_use_native() {
        let _bguard = crate::ipc::bridge_client::BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
        std::env::set_var("PLAYCUA_BRIDGE_BIN", "/nonexistent/playcua-bridge");
        let ports = build_ports(
            &selected(ModalityKind::Sandbox, true, "backend=direct"),
            Arc::new(NoopCapture),
            Arc::new(NoopInput),
            Arc::new(NoopWindow),
        );
        let err = ports
            .capture
            .capture_display(0)
            .await
            .expect_err("must not use native capture");
        let msg = err.to_string();
        assert!(
            msg.contains("bridge") || msg.contains("PLAYCUA_BRIDGE_BIN"),
            "expected bridge fail-loud, got: {msg}"
        );
        match prev {
            Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
            None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
        }
    }

    #[tokio::test]
    async fn sandbox_bridge_ports_round_trip_injected_client() {
        use crate::adapters::sandbox_bridge::SandboxBridgePorts;
        use crate::ipc::bridge_client::BridgeClient;
        use crate::ipc::mod_types::{read_request, write_response, Response};
        use serde_json::json;
        use tokio::io::BufReader;

        let (client, mut peer) = BridgeClient::duplex_pair(64 * 1024);
        let bridge = SandboxBridgePorts::with_client(Arc::new(client));
        // Simulate what build_sandbox_ports does with a live bridge.
        let capture = bridge.capture();
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut peer);
            let req = read_request(&mut reader).await.unwrap().unwrap();
            assert_eq!(req.method, "screenshot");
            write_response(
                &mut peer,
                &Response::ok(
                    req.id,
                    json!({"data":"YQ==","width":1,"height":1,"format":"png"}),
                ),
            )
            .await
            .unwrap();
        });
        let frame = capture.capture_display(0).await.expect("bridged capture");
        assert_eq!(frame.width, 1);
        server.await.unwrap();
    }
}
