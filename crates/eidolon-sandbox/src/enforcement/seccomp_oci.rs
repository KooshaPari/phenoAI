//! OCI / Docker-style seccomp JSON parsing (hermetic, always-on).
//!
//! Accepts the moby/OCI runtime profile shape:
//! `{ "defaultAction": "SCMP_ACT_ERRNO", "syscalls": [ { "names": [...], "action": "SCMP_ACT_ALLOW", ... } ] }`.
//!
//! Unconditional allows only: entries with `args`, `includes`, or `excludes` are
//! skipped (Docker gates those on caps/arches). Fail-loud when the document is
//! not JSON, missing `syscalls`, or yields an empty allowlist after filtering.

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// Embedded Docker-default-like allowlist (trimmed unconditional allows + a
/// few practical extras: `clone`/`clone3`/`socket`/`arch_prctl`/`personality`).
/// Source lineage: moby `profiles/seccomp/default.json` (v27.3.1), simplified.
pub const OCI_DEFAULT_JSON: &str = include_str!("oci_default.json");

#[derive(Debug, Deserialize)]
struct OciProfileDoc {
    #[serde(rename = "defaultAction")]
    default_action: Option<String>,
    syscalls: Option<Vec<OciSyscallGroup>>,
}

#[derive(Debug, Deserialize)]
struct OciSyscallGroup {
    names: Option<Vec<String>>,
    action: Option<String>,
    #[serde(default)]
    args: Option<serde_json::Value>,
    #[serde(default)]
    includes: Option<serde_json::Value>,
    #[serde(default)]
    excludes: Option<serde_json::Value>,
}

/// Parse OCI/Docker seccomp JSON bytes into unconditional `SCMP_ACT_ALLOW` names.
pub fn parse_oci_allowlist(bytes: &[u8]) -> Result<Vec<String>> {
    let doc: OciProfileDoc = serde_json::from_slice(bytes).map_err(|e| {
        PhenoError::BadRequest(format!(
            "invalid OCI/Docker seccomp JSON: {e} \
             (expected defaultAction + syscalls[].names/action)"
        ))
    })?;

    if doc.syscalls.is_none() {
        return Err(PhenoError::BadRequest(
            "OCI seccomp profile missing required `syscalls` array".into(),
        ));
    }

    if let Some(action) = doc.default_action.as_deref() {
        let ok = matches!(
            action,
            "SCMP_ACT_ERRNO" | "SCMP_ACT_KILL" | "SCMP_ACT_KILL_PROCESS" | "SCMP_ACT_TRAP"
        );
        if !ok {
            return Err(PhenoError::BadRequest(format!(
                "OCI seccomp defaultAction {action:?} is not a deny-style action \
                 (need SCMP_ACT_ERRNO / KILL / TRAP for an allowlist profile)"
            )));
        }
    }

    let mut names = BTreeSet::new();
    for group in doc.syscalls.unwrap_or_default() {
        let action = group.action.as_deref().unwrap_or("");
        if action != "SCMP_ACT_ALLOW" {
            continue;
        }
        // Skip conditional groups — Docker uses these for caps / arches / args.
        if group.args.is_some() || group.includes.is_some() || group.excludes.is_some() {
            continue;
        }
        for name in group.names.unwrap_or_default() {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                names.insert(trimmed.to_string());
            }
        }
    }

    if names.is_empty() {
        return Err(PhenoError::BadRequest(
            "OCI seccomp profile produced an empty unconditional allowlist \
             (need at least one SCMP_ACT_ALLOW group without args/includes/excludes)"
                .into(),
        ));
    }

    Ok(names.into_iter().collect())
}

/// Load and parse an OCI profile from a filesystem path.
pub fn load_oci_allowlist_file(path: &Path) -> Result<Vec<String>> {
    let bytes = std::fs::read(path).map_err(|e| {
        PhenoError::BadRequest(format!(
            "cannot read seccomp profile {}: {e}",
            path.display()
        ))
    })?;
    parse_oci_allowlist(&bytes).map_err(|e| match e {
        PhenoError::BadRequest(msg) => PhenoError::BadRequest(format!(
            "seccomp profile {}: {msg}",
            path.display()
        )),
        other => other,
    })
}

/// Embedded `oci-default` allowlist names.
pub fn oci_default_allowlist() -> Result<Vec<String>> {
    parse_oci_allowlist(OCI_DEFAULT_JSON.as_bytes())
}

/// Convert an allowlist of syscall names into seccompiler JSON (json feature).
///
/// Used on Linux apply to build BPF via `seccompiler::compile_from_json`.
pub fn to_seccompiler_allowlist_json(names: &[String]) -> String {
    let filter: Vec<serde_json::Value> = names
        .iter()
        .map(|n| serde_json::json!({ "syscall": n }))
        .collect();
    serde_json::json!({
        "eidolon": {
            "mismatch_action": { "errno": 1 },
            "match_action": "allow",
            "filter": filter
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn embedded_oci_default_parses_nonempty() {
        let names = oci_default_allowlist().expect("embedded oci-default");
        assert!(names.len() >= 300, "got {}", names.len());
        assert!(names.iter().any(|n| n == "read"));
        assert!(names.iter().any(|n| n == "write"));
        assert!(names.iter().any(|n| n == "clone"));
        assert!(names.iter().any(|n| n == "socket"));
    }

    #[test]
    fn rejects_empty_allowlist() {
        let json = br#"{ "defaultAction": "SCMP_ACT_ERRNO", "syscalls": [] }"#;
        let err = parse_oci_allowlist(json).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn rejects_allow_default_action() {
        let json = br#"{
            "defaultAction": "SCMP_ACT_ALLOW",
            "syscalls": [{ "names": ["read"], "action": "SCMP_ACT_ALLOW" }]
        }"#;
        let err = parse_oci_allowlist(json).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn skips_conditional_groups_keeps_unconditional() {
        let json = br#"{
            "defaultAction": "SCMP_ACT_ERRNO",
            "syscalls": [
                { "names": ["read", "write"], "action": "SCMP_ACT_ALLOW" },
                {
                    "names": ["mount"],
                    "action": "SCMP_ACT_ALLOW",
                    "includes": { "caps": ["CAP_SYS_ADMIN"] }
                },
                {
                    "names": ["socket"],
                    "action": "SCMP_ACT_ALLOW",
                    "args": [{ "index": 0, "value": 40, "op": "SCMP_CMP_NE" }]
                }
            ]
        }"#;
        let names = parse_oci_allowlist(json).unwrap();
        assert_eq!(names, vec!["read".to_string(), "write".to_string()]);
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_oci_allowlist(b"not-json").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn load_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "eidolon-seccomp-oci-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("profile.json");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(
                br#"{ "defaultAction": "SCMP_ACT_ERRNO",
                   "syscalls": [{ "names": ["openat"], "action": "SCMP_ACT_ALLOW" }] }"#,
            )
            .unwrap();
        }
        let names = load_oci_allowlist_file(&path).unwrap();
        assert_eq!(names, vec!["openat".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seccompiler_json_shape() {
        let j = to_seccompiler_allowlist_json(&["read".into(), "write".into()]);
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["eidolon"]["match_action"], "allow");
        assert_eq!(v["eidolon"]["mismatch_action"]["errno"], 1);
        assert_eq!(v["eidolon"]["filter"].as_array().unwrap().len(), 2);
    }
}
