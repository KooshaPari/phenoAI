//! Plan → backend argv / Firecracker config (compose; no second VirtualStage).
//!
//! Always-on pure builders so macOS CI can assert argv + JSON without
//! Firecracker, `ops`, `/dev/kvm`, or a real rootfs. Live process spawn is
//! env-gated ([`BOOT_INTEGRATION_ENV`] / [`FIRECRACKER_INTEGRATION_ENV`] /
//! [`NANOVM_BOOT_INTEGRATION_ENV`]) and lives in feature-gated clients.

use super::{LaunchPlan, UnikernelBackend};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Env gate for destructive unikernel / microVM guest process boot.
///
/// When unset (default), hermetic `start` validates artifacts + CLI only.
pub const BOOT_INTEGRATION_ENV: &str = "UNIKERNEL_BOOT_INTEGRATION";

/// Alias env gate preferred by Firecracker / KVM integration paths.
pub const FIRECRACKER_INTEGRATION_ENV: &str = "FIRECRACKER_INTEGRATION";

/// Env gate for NanoVMs `ops` live image boot (shared with nanovm tests).
pub const NANOVM_BOOT_INTEGRATION_ENV: &str = "NANOVM_INTEGRATION";

/// Default Firecracker kernel boot args when [`LaunchPlan::boot_args`] is empty.
pub const DEFAULT_FIRECRACKER_BOOT_ARGS: &str =
    "console=ttyS0 reboot=k panic=1 pci=off";

/// `true` when a live guest-boot integration env is set to `"1"`.
pub fn boot_integration_enabled() -> bool {
    env_is_one(BOOT_INTEGRATION_ENV)
        || env_is_one(FIRECRACKER_INTEGRATION_ENV)
        || env_is_one(NANOVM_BOOT_INTEGRATION_ENV)
}

/// `true` when Firecracker-specific live boot is requested.
pub fn firecracker_boot_enabled() -> bool {
    env_is_one(BOOT_INTEGRATION_ENV) || env_is_one(FIRECRACKER_INTEGRATION_ENV)
}

/// `true` when NanoVMs live boot is requested.
pub fn nanovm_boot_enabled() -> bool {
    env_is_one(BOOT_INTEGRATION_ENV) || env_is_one(NANOVM_BOOT_INTEGRATION_ENV)
}

fn env_is_one(key: &str) -> bool {
    std::env::var(key).ok().as_deref() == Some("1")
}

/// Resolved Firecracker kernel path from a plan (required for microVM boot).
pub fn require_firecracker_kernel(plan: &LaunchPlan) -> Result<&Path> {
    plan.kernel.as_deref().ok_or_else(|| {
        PhenoError::unsupported_platform(
            codes::SANDBOX_KERNEL_MISSING,
            "Firecracker guest boot requires a kernel image (KernelConfig / \
             EIDOLON_KERNEL); fail-loud — do not invent a default vmlinux \
             (docs/EXTRACTION_PLAN.md; do not unarchive KDesktopVirt routinely)",
        )
    })
}

/// Serialize a [`LaunchPlan`] into Firecracker `--config-file` JSON.
///
/// Fail-loud when backend is not Firecracker or kernel is missing.
pub fn firecracker_config_json(plan: &LaunchPlan) -> Result<String> {
    if plan.backend != UnikernelBackend::Firecracker {
        return Err(PhenoError::BadRequest(format!(
            "firecracker_config_json requires UnikernelBackend::Firecracker \
             (got {:?})",
            plan.backend.as_str()
        )));
    }
    let kernel = require_firecracker_kernel(plan)?;
    let boot_args = if plan.boot_args.trim().is_empty() {
        DEFAULT_FIRECRACKER_BOOT_ARGS
    } else {
        plan.boot_args.as_str()
    };
    // Manual JSON — keep serde_json for round-trip tests without inventing a
    // Firecracker schema crate. Paths must be UTF-8 for the config file.
    let kernel_s = path_utf8(kernel, "kernel")?;
    let rootfs_s = path_utf8(&plan.rootfs, "rootfs")?;
    let boot_esc = json_escape(boot_args);
    Ok(format!(
        r#"{{
  "boot-source": {{
    "kernel_image_path": "{kernel}",
    "boot_args": "{boot}"
  }},
  "drives": [
    {{
      "drive_id": "rootfs",
      "path_on_host": "{rootfs}",
      "is_root_device": true,
      "is_read_only": false
    }}
  ],
  "machine-config": {{
    "vcpu_count": {vcpu},
    "mem_size_mib": {mem},
    "smt": false
  }}
}}"#,
        kernel = kernel_s,
        boot = boot_esc,
        rootfs = rootfs_s,
        vcpu = plan.vcpu_count,
        mem = plan.memory_mib,
    ))
}

/// Write Firecracker config JSON for `plan` to `path`.
pub fn write_firecracker_config(plan: &LaunchPlan, path: &Path) -> Result<()> {
    let json = firecracker_config_json(plan)?;
    std::fs::write(path, json.as_bytes()).map_err(|e| {
        PhenoError::unsupported_platform(
            codes::SANDBOX_UNIKERNEL_STUB,
            format!(
                "failed to write Firecracker config at {}: {e}",
                path.display()
            ),
        )
    })
}

/// Build Firecracker argv: `<bin> --no-api --config-file <config>`.
pub fn firecracker_argv(bin: &Path, config_path: &Path) -> Vec<OsString> {
    vec![
        bin.as_os_str().to_os_string(),
        OsString::from("--no-api"),
        OsString::from("--config-file"),
        config_path.as_os_str().to_os_string(),
    ]
}

/// Build NanoVMs `ops run` argv from a plan (compose; no VirtualStage).
///
/// Shape: `<ops> run <rootfs> -m <memory_mib> -c <vcpu_count>`.
pub fn ops_run_argv(ops: &Path, plan: &LaunchPlan) -> Result<Vec<OsString>> {
    if plan.backend != UnikernelBackend::NanoVm {
        return Err(PhenoError::BadRequest(format!(
            "ops_run_argv requires UnikernelBackend::NanoVm (got {:?})",
            plan.backend.as_str()
        )));
    }
    Ok(vec![
        ops.as_os_str().to_os_string(),
        OsString::from("run"),
        plan.rootfs.as_os_str().to_os_string(),
        OsString::from("-m"),
        OsString::from(plan.memory_mib.to_string()),
        OsString::from("-c"),
        OsString::from(plan.vcpu_count.to_string()),
    ])
}

/// Spawn a Firecracker guest from `plan` using `bin` (env-gated callers only).
pub fn spawn_firecracker(bin: &Path, plan: &LaunchPlan, config_path: &Path) -> Result<Child> {
    plan.require_ready()?;
    require_firecracker_kernel(plan)?;
    write_firecracker_config(plan, config_path)?;
    let argv = firecracker_argv(bin, config_path);
    spawn_argv(&argv, "firecracker", codes::SANDBOX_KVM_STUB)
}

/// Spawn a NanoVMs `ops run` guest from `plan` (env-gated callers only).
pub fn spawn_ops(ops: &Path, plan: &LaunchPlan) -> Result<Child> {
    plan.require_ready()?;
    let argv = ops_run_argv(ops, plan)?;
    spawn_argv(&argv, "ops", codes::SANDBOX_NANOVM_STUB)
}

fn spawn_argv(argv: &[OsString], label: &str, code: &'static str) -> Result<Child> {
    let Some((prog, args)) = argv.split_first() else {
        return Err(PhenoError::BadRequest(format!(
            "{label} guest boot argv is empty"
        )));
    };
    // Pipe stdin/stdout for post-boot serial/stdio guest exec
    // ([`super::exec`]); stderr stays piped for diagnostics.
    Command::new(prog)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            PhenoError::unsupported_platform(
                code,
                format!(
                    "{label} guest process spawn failed ({e}); set \
                     {BOOT_INTEGRATION_ENV}=1 / {FIRECRACKER_INTEGRATION_ENV}=1 \
                     only on hosts with tools+rootfs+kernel; \
                     docs/EXTRACTION_PLAN.md; do not unarchive KDesktopVirt \
                     routinely"
                ),
            )
        })
}

/// Default temp path for a Firecracker config belonging to `sandbox_id`.
pub fn default_firecracker_config_path(sandbox_id: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "eidolon-fc-{}-{}.json",
        sandbox_id,
        std::process::id()
    ));
    path
}

fn path_utf8(path: &Path, kind: &str) -> Result<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| PhenoError::BadRequest(format!("unikernel {kind} path must be UTF-8")))
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unikernel::{KernelConfig, RootfsConfig, UnikernelLaunchConfig};
    use std::io::Write;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_file(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-boot-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut f = std::fs::File::create(&path).expect("create");
        writeln!(f, "boot-fixture").expect("write");
        path
    }

    fn plan_fc(rootfs: &Path, kernel: &Path) -> LaunchPlan {
        // Bypass CLI probe for argv/JSON unit tests: construct LaunchPlan
        // fields directly (macOS-safe; no firecracker required).
        LaunchPlan {
            sandbox_id: "boot-fc".into(),
            backend: UnikernelBackend::Firecracker,
            rootfs: rootfs.to_path_buf(),
            rootfs_format: super::super::RootfsFormat::Ext4,
            kernel: Some(kernel.to_path_buf()),
            boot_args: String::new(),
            vcpu_count: 2,
            memory_mib: 256,
            policy: eidolon_core::security::SandboxPolicy::default(),
        }
    }

    #[test]
    fn firecracker_argv_shape() {
        let bin = Path::new("/usr/bin/firecracker");
        let cfg = Path::new("/tmp/vm.json");
        let argv = firecracker_argv(bin, cfg);
        assert_eq!(
            argv,
            vec![
                OsString::from("/usr/bin/firecracker"),
                OsString::from("--no-api"),
                OsString::from("--config-file"),
                OsString::from("/tmp/vm.json"),
            ]
        );
    }

    #[test]
    fn firecracker_config_includes_kernel_rootfs_resources() {
        let rootfs = temp_file("rootfs");
        let kernel = temp_file("vmlinux");
        let plan = plan_fc(&rootfs, &kernel);
        let json = firecracker_config_json(&plan).expect("json");
        assert!(json.contains(&format!("\"kernel_image_path\": \"{}\"", kernel.display())));
        assert!(json.contains(&format!("\"path_on_host\": \"{}\"", rootfs.display())));
        assert!(json.contains("\"vcpu_count\": 2"));
        assert!(json.contains("\"mem_size_mib\": 256"));
        assert!(json.contains(DEFAULT_FIRECRACKER_BOOT_ARGS));
        let _ = std::fs::remove_file(&rootfs);
        let _ = std::fs::remove_file(&kernel);
    }

    #[test]
    fn firecracker_config_missing_kernel_fail_loud() {
        let rootfs = temp_file("rootfs-nok");
        let plan = LaunchPlan {
            sandbox_id: "boot-nok".into(),
            backend: UnikernelBackend::Firecracker,
            rootfs: rootfs.clone(),
            rootfs_format: super::super::RootfsFormat::Ext4,
            kernel: None,
            boot_args: String::new(),
            vcpu_count: 1,
            memory_mib: 128,
            policy: eidolon_core::security::SandboxPolicy::default(),
        };
        let err = firecracker_config_json(&plan).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_KERNEL_MISSING));
        let _ = std::fs::remove_file(&rootfs);
    }

    #[test]
    fn ops_run_argv_shape() {
        let rootfs = temp_file("ops-pkg");
        let plan = LaunchPlan {
            sandbox_id: "boot-ops".into(),
            backend: UnikernelBackend::NanoVm,
            rootfs: rootfs.clone(),
            rootfs_format: super::super::RootfsFormat::OpsPackage,
            kernel: None,
            boot_args: String::new(),
            vcpu_count: 1,
            memory_mib: 128,
            policy: eidolon_core::security::SandboxPolicy::default(),
        };
        let ops = Path::new("/usr/local/bin/ops");
        let argv = ops_run_argv(ops, &plan).expect("argv");
        assert_eq!(
            argv,
            vec![
                OsString::from("/usr/local/bin/ops"),
                OsString::from("run"),
                rootfs.as_os_str().to_os_string(),
                OsString::from("-m"),
                OsString::from("128"),
                OsString::from("-c"),
                OsString::from("1"),
            ]
        );
        let _ = std::fs::remove_file(&rootfs);
    }

    #[test]
    fn ops_run_rejects_firecracker_backend() {
        let rootfs = temp_file("ops-wrong");
        let kernel = temp_file("ops-wrong-k");
        let plan = plan_fc(&rootfs, &kernel);
        let err = ops_run_argv(Path::new("ops"), &plan).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
        let _ = std::fs::remove_file(&rootfs);
        let _ = std::fs::remove_file(&kernel);
    }

    #[test]
    fn missing_rootfs_via_launch_config_fail_loud() {
        let cfg = UnikernelLaunchConfig::try_new(
            "boot-miss",
            UnikernelBackend::Firecracker,
            RootfsConfig::new("/no/such/eidolon-boot-rootfs.img"),
        )
        .unwrap();
        let err = LaunchPlan::try_from_config(&cfg).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_ROOTFS_MISSING));
    }

    #[test]
    fn missing_kernel_via_launch_config_fail_loud() {
        let rootfs = temp_file("cfg-rootfs");
        let cfg = UnikernelLaunchConfig::try_new(
            "boot-kern-miss",
            UnikernelBackend::Firecracker,
            RootfsConfig::new(&rootfs),
        )
        .unwrap()
        .with_kernel(KernelConfig::new("/no/such/eidolon-boot-vmlinux"))
        .unwrap();
        let err = LaunchPlan::try_from_config(&cfg).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_KERNEL_MISSING));
        let _ = std::fs::remove_file(&rootfs);
    }

    #[test]
    fn boot_integration_env_gate() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(BOOT_INTEGRATION_ENV);
        std::env::remove_var(FIRECRACKER_INTEGRATION_ENV);
        std::env::remove_var(NANOVM_BOOT_INTEGRATION_ENV);
        assert!(!boot_integration_enabled());
        std::env::set_var(FIRECRACKER_INTEGRATION_ENV, "1");
        assert!(boot_integration_enabled());
        assert!(firecracker_boot_enabled());
        std::env::remove_var(FIRECRACKER_INTEGRATION_ENV);
        std::env::set_var(BOOT_INTEGRATION_ENV, "1");
        assert!(nanovm_boot_enabled());
        std::env::remove_var(BOOT_INTEGRATION_ENV);
    }
}
