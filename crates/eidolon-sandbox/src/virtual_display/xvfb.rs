//! Xvfb virtual X11 display (Linux live path).
//!
//! Hermetic: probe + argv planning without spawn unless
//! [`super::XVFB_INTEGRATION_ENV`]=`1` and feature `sandbox-virtual-display`.

use super::probe;
use super::{
    platform_unsupported, xvfb_missing, VirtualDisplayConfig, VirtualDisplayHandle,
    VirtualDisplayKind, XVFB_INTEGRATION_ENV,
};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

fn xvfb_spawn_failed(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::Internal(format!(
        "{} — Xvfb spawn failed: {detail} (see docs/guides/virtual-display.md)",
        codes::SANDBOX_XVFB_SPAWN_IO
    ))
}

/// Planned argv for an Xvfb instance (hermetic — no spawn).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XvfbLaunchPlan {
    pub binary: PathBuf,
    pub display: String,
    pub argv: Vec<String>,
}

/// Build an Xvfb launch plan (hermetic; validates binary presence on Linux).
pub fn plan_xvfb(config: &VirtualDisplayConfig) -> Result<XvfbLaunchPlan> {
    if !super::platform_is_linux() {
        return Err(platform_unsupported("plan_xvfb"));
    }
    let Some(binary) = probe::resolve_xvfb_cli() else {
        return Err(xvfb_missing("plan_xvfb"));
    };
    let screen = format!("{}x{}x{}", config.width, config.height, config.color_depth);
    let display = config.display_string();
    let argv = vec![
        display.clone(),
        "-screen".into(),
        "0".into(),
        screen,
        "-ac".into(),
        "+extension".into(),
        "GLX".into(),
        "+render".into(),
        "-noreset".into(),
    ];
    Ok(XvfbLaunchPlan {
        binary,
        display,
        argv,
    })
}

/// Whether live Xvfb spawn is allowed (feature + env gate).
pub fn live_spawn_enabled() -> bool {
    cfg!(feature = "sandbox-virtual-display")
        && super::platform_is_linux()
        && integration_env_set(XVFB_INTEGRATION_ENV)
}

fn integration_env_set(key: &str) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.trim(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

/// Spawn Xvfb when [`live_spawn_enabled`] is true; otherwise fail-loud stub.
pub fn spawn_xvfb(config: &VirtualDisplayConfig) -> Result<(VirtualDisplayHandle, Child)> {
    if !super::platform_is_linux() {
        return Err(platform_unsupported("spawn_xvfb"));
    }
    if !live_spawn_enabled() {
        return Err(PhenoError::unsupported_platform(
            codes::SANDBOX_VIRTUAL_DISPLAY_STUB,
            format!(
                "Xvfb live spawn gated — enable feature `sandbox-virtual-display`, run on \
                 Linux, and set {XVFB_INTEGRATION_ENV}=1 (hermetic plan_xvfb still works; \
                 docs/guides/virtual-display.md)"
            ),
        ));
    }
    let plan = plan_xvfb(config)?;
    let mut cmd = Command::new(&plan.binary);
    cmd.args(&plan.argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| xvfb_spawn_failed(format!("{}: {e}", plan.binary.display())))?;
    let handle = VirtualDisplayHandle {
        kind: VirtualDisplayKind::Xvfb,
        display: plan.display,
        width: config.width,
        height: config.height,
        color_depth: config.color_depth,
        vnc_port: None,
        wayland_socket: None,
    };
    Ok((handle, child))
}

/// Best-effort terminate an Xvfb child.
pub fn stop_xvfb_child(mut child: Child) -> Result<()> {
    if let Err(e) = child.kill() {
        log::debug!("Xvfb child kill: {e}");
    }
    let _ = child.wait();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_xvfb_fail_loud_off_linux_or_missing() {
        let config = VirtualDisplayConfig::default();
        match plan_xvfb(&config) {
            Ok(plan) => {
                assert!(super::super::platform_is_linux());
                assert!(probe::xvfb_ready());
                assert!(plan.display.starts_with(':'));
                assert!(plan.argv.iter().any(|a| a.contains('x')));
            }
            Err(e) => {
                let code = e.unsupported_code();
                assert!(
                    code == Some(codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED)
                        || code == Some(codes::SANDBOX_XVFB_MISSING),
                    "unexpected plan_xvfb error: {e:?}"
                );
            }
        }
    }

    #[test]
    fn spawn_xvfb_fail_loud_without_integration_env() {
        if !super::super::platform_is_linux() || !probe::xvfb_ready() {
            return;
        }
        let saved = std::env::var(XVFB_INTEGRATION_ENV).ok();
        std::env::remove_var(XVFB_INTEGRATION_ENV);
        let err = spawn_xvfb(&VirtualDisplayConfig::default()).unwrap_err();
        if let Some(v) = saved {
            std::env::set_var(XVFB_INTEGRATION_ENV, v);
        }
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_VIRTUAL_DISPLAY_STUB)
        );
    }
}
