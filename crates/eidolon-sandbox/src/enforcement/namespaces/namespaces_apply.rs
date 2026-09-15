//! Linux-specific namespace apply / fork / signal logic.

use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use nix::sched::{unshare, CloneFlags};
// wraps: nix 0.31 — sched::unshare / CloneFlags, unistd::fork/getpid,
//         signal::{kill,signal,sigaction}, sys::wait::{waitpid,WaitPidFlag}
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, signal, SaFlags, SigAction, SigHandler, SigSet, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, getpid, pipe, ForkResult, Pid};

use super::*;
use crate::codes;
use crate::enforcement::namespaces_pivot;
use crate::enforcement::plan::{NamespacePlan, UserNsIdMap};

/// How long `terminate_child` waits for SIGTERM before escalating to SIGKILL.
const TERM_GRACE: Duration = Duration::from_millis(500);
const TERM_POLL: Duration = Duration::from_millis(20);

pub(super) fn apply_plan(plan: &NamespacePlan) -> Result<NamespaceApply> {
    validate_pivot_plan(plan)?;
    let (flags, names) = flags_from_plan(plan)?;
    unshare(flags).map_err(|e| {
        unsupported(format!(
            "unshare({names:?}) failed: {e} \
                 (need CAP_SYS_ADMIN or a delegated user namespace)"
        ))
    })?;
    let mapped = maybe_map_user(plan)?;

    if !plan.pid {
        let pivoted = maybe_pivot(plan)?;
        return Ok(NamespaceApply {
            status: NamespaceStatus {
                flags_applied: names,
                pid_ns_isolates_caller: false,
                isolated_child_host_pid: None,
                pivot_root_applied: pivoted,
                user_ns_mapped: mapped,
            },
            setup: None,
        });
    }

    // PID ns: caller is still in the old ns until fork — child becomes PID 1.
    // Pipe handshake: child reports later-hook success/failure before parent
    // returns Ok from wiring. Pivot runs in the child only.
    let (read_end, write_end) = pipe_pair()?;
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            drop(write_end);
            Ok(NamespaceApply {
                status: NamespaceStatus {
                    flags_applied: names.clone(),
                    pid_ns_isolates_caller: true,
                    isolated_child_host_pid: Some(child.as_raw() as u32),
                    // Parent keeps host root; child applies pivot.
                    pivot_root_applied: false,
                    // Maps were written in this process before fork.
                    user_ns_mapped: mapped,
                },
                setup: Some(PidNsSetup::Parent(PidNsParentSetup {
                    read_end,
                    child_host_pid: child.as_raw() as u32,
                })),
            })
        }
        Ok(ForkResult::Child) => {
            drop(read_end);
            // Inherited ignored/custom SIGTERM would make stop()/teardown hang.
            reset_child_termination_signals();
            let pivoted = match maybe_pivot(plan) {
                Ok(v) => v,
                Err(e) => {
                    // Fail loud before returning to wiring — parent await will see pipe fail.
                    let _ = write_end;
                    return Err(e);
                }
            };
            Ok(NamespaceApply {
                status: NamespaceStatus {
                    flags_applied: names,
                    pid_ns_isolates_caller: true,
                    isolated_child_host_pid: None,
                    pivot_root_applied: pivoted,
                    user_ns_mapped: mapped,
                },
                setup: Some(PidNsSetup::Child(PidNsChildSetup { write_end })),
            })
        }
        Err(e) => Err(unsupported(format!(
            "fork after unshare(CLONE_NEWPID) failed: {e} \
                 (PID ns isolation requires fork; refusing silent skip)"
        ))),
    }
}

pub(super) fn run_in_plan(
    plan: &NamespacePlan,
    work: impl FnOnce() -> Result<()>,
) -> Result<NamespaceStatus> {
    validate_pivot_plan(plan)?;
    if !plan.pid {
        let applied = apply_plan(plan)?;
        work()?;
        return Ok(applied.status);
    }

    let (flags, names) = flags_from_plan(plan)?;
    let (mut read_end, mut write_end) = pipe_pair()?;
    let want_pivot = plan.pivot_rootfs.is_some();

    unshare(flags).map_err(|e| {
        unsupported(format!(
            "unshare({names:?}) failed: {e} \
                 (need CAP_SYS_ADMIN or a delegated user namespace)"
        ))
    })?;
    let mapped = maybe_map_user(plan)?;
    let want_user = plan.user;

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            drop(write_end);
            let mut buf = [0u8; 1];
            let n = read_end
                .read(&mut buf)
                .map_err(|e| unsupported(format!("read PID-ns child pipe: {e}")))?;
            match waitpid(child, None) {
                Ok(WaitStatus::Exited(_, code)) if code == 0 && n == 1 && buf[0] == 0 => {
                    Ok(NamespaceStatus {
                        flags_applied: names,
                        pid_ns_isolates_caller: true,
                        isolated_child_host_pid: Some(child.as_raw() as u32),
                        // Child pivoted (when planned) before work succeeded.
                        pivot_root_applied: want_pivot,
                        user_ns_mapped: want_user && mapped,
                    })
                }
                Ok(WaitStatus::Exited(_, code)) => Err(unsupported(format!(
                    "PID-ns child failed (exit={code}, pipe_byte={})",
                    if n == 1 { buf[0] } else { 255 }
                ))),
                Ok(other) => Err(unsupported(format!(
                    "PID-ns child did not exit cleanly: {other:?}"
                ))),
                Err(e) => Err(unsupported(format!("waitpid PID-ns child: {e}"))),
            }
        }
        Ok(ForkResult::Child) => {
            drop(read_end);
            reset_child_termination_signals();
            let pid = getpid().as_raw();
            let ok = pid == 1 && maybe_pivot(plan).is_ok() && work().is_ok();
            let _ = write_end.write_all(&[if ok { 0 } else { 1 }]);
            let _ = write_end.flush();
            // Safety: child must not unwind into parent's Rust runtime.
            unsafe { libc::_exit(if ok { 0 } else { 1 }) }
        }
        Err(e) => Err(unsupported(format!(
            "fork after unshare(CLONE_NEWPID) failed: {e}"
        ))),
    }
}

fn validate_pivot_plan(plan: &NamespacePlan) -> Result<()> {
    if plan.pivot_rootfs.is_some() && !plan.mount {
        return Err(PhenoError::BadRequest(
            "NamespacePlan.pivot_rootfs requires mount=true (CLONE_NEWNS \
                 before pivot_root); refusing silent skip"
                .into(),
        ));
    }
    Ok(())
}

fn maybe_pivot(plan: &NamespacePlan) -> Result<bool> {
    let Some(rootfs) = plan.pivot_rootfs.as_ref() else {
        return Ok(false);
    };
    namespaces_pivot::pivot_into(rootfs)?;
    Ok(true)
}

fn maybe_map_user(plan: &NamespacePlan) -> Result<bool> {
    if !plan.user {
        return Ok(false);
    }
    let uid_maps = if plan.uid_maps.is_empty() {
        vec![UserNsIdMap {
            inside: 0,
            outside: unsafe { libc::geteuid() },
            count: 1,
        }]
    } else {
        plan.uid_maps.clone()
    };
    let gid_maps = if plan.gid_maps.is_empty() {
        vec![UserNsIdMap {
            inside: 0,
            outside: unsafe { libc::getegid() },
            count: 1,
        }]
    } else {
        plan.gid_maps.clone()
    };
    for m in uid_maps.iter().chain(gid_maps.iter()) {
        if m.count == 0 {
            return Err(PhenoError::BadRequest(
                "USER ns uid/gid map count must be >= 1; refusing silent skip".into(),
            ));
        }
    }
    crate::enforcement::namespaces_user::write_maps(&uid_maps, &gid_maps)?;
    Ok(true)
}

pub(super) fn await_pid_ns_setup(read_end: &mut std::fs::File, child_host_pid: u32) -> Result<()> {
    let mut buf = [0u8; 1];
    let n = read_end
        .read(&mut buf)
        .map_err(|e| unsupported(format!("read PID-ns setup pipe: {e}")))?;
    if n == 1 && buf[0] == 0 {
        return Ok(());
    }
    // Child failed or died before reporting success — reap to avoid zombies.
    let child = Pid::from_raw(child_host_pid as i32);
    let _ = waitpid(child, None);
    Err(unsupported(format!(
        "PID-ns child setup failed (pipe_byte={})",
        if n == 1 { buf[0] } else { 255 }
    )))
}

pub(super) fn report_pid_ns_setup(write_end: &mut std::fs::File, ok: bool) -> Result<()> {
    write_end
        .write_all(&[if ok { 0 } else { 1 }])
        .map_err(|e| unsupported(format!("write PID-ns setup pipe: {e}")))?;
    write_end
        .flush()
        .map_err(|e| unsupported(format!("flush PID-ns setup pipe: {e}")))?;
    Ok(())
}

/// SIGTERM then bounded poll; escalate to SIGKILL + waitpid so stop() cannot
/// hang if the child inherited an ignored/custom SIGTERM disposition.
pub(super) fn terminate_child(host_pid: u32) -> Result<()> {
    let pid = Pid::from_raw(host_pid as i32);
    match kill(pid, Signal::SIGTERM) {
        Ok(()) => {}
        // Already gone — treat as clean teardown.
        Err(nix::errno::Errno::ESRCH) => return Ok(()),
        Err(e) => {
            return Err(unsupported(format!(
                "kill({host_pid}, SIGTERM) for PID-ns child failed: {e}"
            )));
        }
    }

    let deadline = Instant::now() + TERM_GRACE;
    loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(TERM_POLL);
            }
            Ok(_) => return Ok(()),
            Err(nix::errno::Errno::ECHILD) => return Ok(()),
            Err(e) => {
                return Err(unsupported(format!(
                    "waitpid({host_pid}) after SIGTERM failed: {e}"
                )));
            }
        }
    }

    match kill(pid, Signal::SIGKILL) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
        Err(e) => {
            return Err(unsupported(format!(
                "kill({host_pid}, SIGKILL) for PID-ns child failed: {e}"
            )));
        }
    }
    match waitpid(pid, None) {
        Ok(_) | Err(nix::errno::Errno::ECHILD) => Ok(()),
        Err(e) => Err(unsupported(format!(
            "waitpid({host_pid}) after SIGKILL failed: {e}"
        ))),
    }
}

pub(super) fn park_until_signal() -> ! {
    // Default disposition: terminating signals end the process (no handler
    // loop). pause() returns only for non-terminating caught signals.
    loop {
        nix::unistd::pause();
    }
}

/// Reset SIGTERM/SIGINT/SIGQUIT to SIG_DFL so parent `stop()` can terminate
/// a parked child even when the host ignored or trapped those signals.
fn reset_child_termination_signals() {
    let action = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
    for sig in [Signal::SIGTERM, Signal::SIGINT, Signal::SIGQUIT] {
        // Best-effort: failure leaves SIGKILL escalation in terminate_child.
        let _ = unsafe { nix::sys::signal::sigaction(sig, &action) };
        let _ = unsafe { signal(sig, SigHandler::SigDfl) };
    }
}

fn flags_from_plan(plan: &NamespacePlan) -> Result<(CloneFlags, Vec<&'static str>)> {
    let mut flags = CloneFlags::empty();
    let mut names: Vec<&'static str> = Vec::new();

    if plan.uts {
        flags |= CloneFlags::CLONE_NEWUTS;
        names.push("uts");
    }
    if plan.ipc {
        flags |= CloneFlags::CLONE_NEWIPC;
        names.push("ipc");
    }
    if plan.cgroup {
        flags |= CloneFlags::CLONE_NEWCGROUP;
        names.push("cgroup");
    }
    if plan.net {
        flags |= CloneFlags::CLONE_NEWNET;
        names.push("net");
    }
    if plan.mount {
        flags |= CloneFlags::CLONE_NEWNS;
        names.push("mount");
    }
    if plan.pid {
        flags |= CloneFlags::CLONE_NEWPID;
        names.push("pid");
    }
    if plan.user {
        flags |= CloneFlags::CLONE_NEWUSER;
        names.push("user");
    }

    if flags.is_empty() {
        return Err(PhenoError::BadRequest(
            "NamespacePlan requests no namespaces (empty unshare is not isolation)".into(),
        ));
    }
    Ok((flags, names))
}

fn pipe_pair() -> Result<(std::fs::File, std::fs::File)> {
    let (read_fd, write_fd) =
        pipe().map_err(|e| unsupported(format!("pipe() for PID-ns IPC failed: {e}")))?;
    // Safety: exclusive ownership of the pipe ends from nix::unistd::pipe.
    Ok(unsafe {
        (
            std::fs::File::from_raw_fd(read_fd.into_raw_fd()),
            std::fs::File::from_raw_fd(write_fd.into_raw_fd()),
        )
    })
}

#[cfg(test)]
mod hermetic {

    #[test]
    fn flags_include_pid_when_requested() {
        let plan = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: true,
            user: false,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: None,
        };
        let (flags, names) = flags_from_plan(&plan).expect("flags");
        assert!(flags.contains(CloneFlags::CLONE_NEWPID));
        assert!(flags.contains(CloneFlags::CLONE_NEWUTS));
        assert_eq!(names, vec!["uts", "pid"]);
    }

    #[test]
    fn flags_include_user_when_requested() {
        let plan = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: false,
            user: true,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: None,
        };
        let (flags, names) = flags_from_plan(&plan).expect("flags");
        assert!(flags.contains(CloneFlags::CLONE_NEWUSER));
        assert_eq!(names, vec!["uts", "user"]);
    }

    #[test]
    fn flags_include_mount_when_pivot_planned() {
        let plan = NamespacePlan {
            uts: false,
            ipc: false,
            cgroup: false,
            net: false,
            mount: true,
            pid: false,
            user: false,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: Some(PathBuf::from("/tmp/rootfs")),
        };
        let (flags, names) = flags_from_plan(&plan).expect("flags");
        assert!(flags.contains(CloneFlags::CLONE_NEWNS));
        assert_eq!(names, vec!["mount"]);
        validate_pivot_plan(&plan).expect("mount+pivot ok");
    }

    #[test]
    fn pivot_without_mount_is_bad_request() {
        let plan = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: false,
            user: false,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: Some(PathBuf::from("/tmp/rootfs")),
        };
        let err = validate_pivot_plan(&plan).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn empty_plan_is_bad_request() {
        let plan = NamespacePlan {
            uts: false,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: false,
            user: false,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: None,
        };
        let err = flags_from_plan(&plan).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn status_honesty_helpers() {
        let parent = NamespaceStatus {
            flags_applied: vec!["pid"],
            pid_ns_isolates_caller: true,
            isolated_child_host_pid: Some(4242),
            pivot_root_applied: false,
            user_ns_mapped: false,
        };
        assert!(parent.is_pid_ns_parent());
        assert!(!parent.is_pid_ns_child());

        let child = NamespaceStatus {
            flags_applied: vec!["pid", "mount"],
            pid_ns_isolates_caller: true,
            isolated_child_host_pid: None,
            pivot_root_applied: true,
            user_ns_mapped: true,
        };
        assert!(!child.is_pid_ns_parent());
        assert!(child.is_pid_ns_child());
        assert!(child.pivot_root_applied);

        let no_pid = NamespaceStatus {
            flags_applied: vec!["uts"],
            pid_ns_isolates_caller: false,
            isolated_child_host_pid: None,
            pivot_root_applied: false,
            user_ns_mapped: false,
        };
        assert!(!no_pid.is_pid_ns_parent());
        assert!(!no_pid.is_pid_ns_child());
    }
}
