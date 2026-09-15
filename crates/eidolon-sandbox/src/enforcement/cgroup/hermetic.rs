//! Hermetic fake-cgroup-fs unit tests (feature `sandbox-cgroup`).

use super::*;
use crate::enforcement::plan::{CgroupPlan, CGROUP_CPU_PERIOD_US};
use std::fs;
use std::path::PathBuf;

fn fake_cgroup_tree() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "eidolon-cgroup-fake-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("cgroup.subtree_control"), b"").unwrap();
    fs::write(root.join("cgroup.procs"), b"").unwrap();
    root
}

fn seed_child_controllers(parent: &std::path::Path, sandbox_id: &str) {
    let child = parent.join(format!("eidolon-{sandbox_id}"));
    fs::create_dir_all(&child).unwrap();
    for name in ["memory.max", "cpu.max", "io.max", "cgroup.procs"] {
        fs::write(child.join(name), b"").unwrap();
    }
}

fn base_plan(disk_mib: Option<u32>) -> CgroupPlan {
    CgroupPlan {
        memory_max_bytes: 64 * 1024 * 1024,
        cpu_quota_us: CGROUP_CPU_PERIOD_US,
        cpu_period_us: CGROUP_CPU_PERIOD_US,
        disk_mib,
        disk_enforced: false,
    }
}

#[test]
fn hermetic_memory_cpu_without_disk() {
    let parent = fake_cgroup_tree();
    seed_child_controllers(&parent, "h1");
    let status = apply_plan_at(&parent, "h1", &base_plan(None), None, None).expect("apply");
    assert!(!status.disk_enforced);
    assert!(status.io_max.is_none());
    assert_eq!(status.memory_max_bytes, 64 * 1024 * 1024);
    let mem = fs::read_to_string(parent.join("eidolon-h1/memory.max")).unwrap();
    assert_eq!(mem, (64 * 1024 * 1024).to_string());
    let _ = fs::remove_dir_all(&parent);
}

#[test]
fn hermetic_disk_writes_io_max() {
    let parent = fake_cgroup_tree();
    seed_child_controllers(&parent, "h2");
    let status =
        apply_plan_at(&parent, "h2", &base_plan(Some(100)), Some((8, 0)), None)
            .expect("apply with disk");
    assert!(status.disk_enforced);
    assert_eq!(
        status.io_max.as_deref(),
        Some("8:0 rbps=104857600 wbps=104857600")
    );
    let io = fs::read_to_string(parent.join("eidolon-h2/io.max")).unwrap();
    assert_eq!(io, "8:0 rbps=104857600 wbps=104857600");
    let _ = fs::remove_dir_all(&parent);
}

#[test]
fn hermetic_disk_without_device_fail_loud() {
    let parent = fake_cgroup_tree();
    seed_child_controllers(&parent, "h3");
    let err = apply_plan_at(&parent, "h3", &base_plan(Some(50)), None, None).unwrap_err();
    assert_eq!(
        err.unsupported_code(),
        Some(codes::SANDBOX_CGROUP_DISK_UNAVAILABLE)
    );
    let _ = fs::remove_dir_all(&parent);
}
