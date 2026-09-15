//! Arch-dispatch syscall name → number lookup.
//!
//! wraps/source: seccompiler 0.5 syscall tables (Apache-2.0 OR BSD-3-Clause).
//! Used by Linux `sandbox-seccomp` allowlist apply; compiled everywhere so
//! hermetic unit tests can resolve names off-platform.

#![allow(dead_code)] // consumed by Linux apply path behind `sandbox-seccomp`

#[cfg(any(target_arch = "x86_64", test))]
#[path = "seccomp_syscall_nr_x86_64.rs"]
mod x86_64;
#[cfg(any(target_arch = "aarch64", test))]
#[path = "seccomp_syscall_nr_aarch64.rs"]
mod aarch64;
#[cfg(any(target_arch = "riscv64", test))]
#[path = "seccomp_syscall_nr_riscv64.rs"]
mod riscv64;

/// Resolve `name` for `arch` (`std::env::consts::ARCH` values).
pub fn syscall_nr(arch: &str, name: &str) -> Option<i64> {
    match arch {
        "x86_64" => {
            #[cfg(any(target_arch = "x86_64", test))]
            {
                return x86_64::lookup(name);
            }
            #[cfg(not(any(target_arch = "x86_64", test)))]
            {
                let _ = name;
                None
            }
        }
        "aarch64" => {
            #[cfg(any(target_arch = "aarch64", test))]
            {
                return aarch64::lookup(name);
            }
            #[cfg(not(any(target_arch = "aarch64", test)))]
            {
                let _ = name;
                None
            }
        }
        "riscv64" => {
            #[cfg(any(target_arch = "riscv64", test))]
            {
                return riscv64::lookup(name);
            }
            #[cfg(not(any(target_arch = "riscv64", test)))]
            {
                let _ = name;
                None
            }
        }
        _ => None,
    }
}

/// Map allowlist names to numbers for `arch`, skipping unknown names.
pub fn resolve_allowlist(arch: &str, names: &[String]) -> Vec<(String, i64)> {
    names
        .iter()
        .filter_map(|n| syscall_nr(arch, n).map(|nr| (n.clone(), nr)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_known_on_x86_64() {
        assert_eq!(syscall_nr("x86_64", "read"), Some(0));
        assert_eq!(syscall_nr("x86_64", "write"), Some(1));
        assert_eq!(syscall_nr("x86_64", "nosuch"), None);
    }

    #[test]
    fn resolve_skips_unknown() {
        let names = vec!["read".into(), "not_a_syscall".into(), "write".into()];
        let got = resolve_allowlist("x86_64", &names);
        assert_eq!(got.len(), 2);
    }
}
