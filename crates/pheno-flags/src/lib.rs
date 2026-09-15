//! Environment-driven boolean feature flags.
//!
//! Designed for the L5 #81 fleet pattern: every binary reads its
//! `<PREFIX>_<KEY>` env vars at startup and threads the resulting
//! [`FlagSet`] through the app builder so platform adapters can
//! branch on individual flags via [`FlagSet::is_enabled`].
//!
//! # Truthy / falsy values
//!
//! Truthy (case-insensitive): `1`, `true`, `yes`.
//! Falsy  (case-insensitive): `0`, `false`, `no`.
//!
//! Any other value yields a [`FlagError::InvalidValue`] (the
//! main binary maps this to `AppError::Flag` → exit code 78,
//! `EX_CONFIG`).
//!
//! Unknown keys default to `false` — `is_enabled("FUTURE")` on an
//! empty `FlagSet` simply returns `false`, the safe default for a
//! not-yet-implemented flag.

use std::collections::BTreeMap;

use thiserror::Error;

/// Errors produced while parsing a `FlagSet` from the environment.
#[derive(Debug, Error)]
pub enum FlagError {
    /// An env var matched `<PREFIX>_<KEY>` but its value was not one
    /// of the recognized truthy/falsy strings.
    #[error("invalid flag value for `{var}`: `{value}` (expected one of 1/true/yes/0/false/no)")]
    InvalidValue { var: String, value: String },
}

/// Bridge to the canonical fleet error type. Lets `?` work in `main`
/// without an extra `.map_err(...)` — the message is preserved verbatim
/// so the original env-var name + value reach the user, and
/// `AppError::Flag` exits with code 78 (`EX_CONFIG`).
impl From<FlagError> for pheno_errors::AppError {
    fn from(e: FlagError) -> Self {
        pheno_errors::AppError::Flag(e.to_string())
    }
}

/// A loaded set of boolean feature flags, keyed by the suffix after
/// `<PREFIX>_`. Use [`FlagSet::from_env`] to load from process env.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagSet {
    flags: BTreeMap<String, bool>,
}

impl FlagSet {
    /// Suffixes under `<PREFIX>_` that are **config strings**, not boolean
    /// flags. Skipped by [`FlagSet::from_env`] so modality/sandbox/nvms
    /// env vars can coexist with `PLAYCUA_VERBOSE` / `PLAYCUA_DRY_RUN`.
    const NON_FLAG_SUFFIXES: &'static [&'static str] =
        &["MODALITY", "SANDBOX_BACKEND", "NVMS_CONFIG"];

    /// Load every `<PREFIX>_<KEY>` env var into a `FlagSet`.
    ///
    /// Iterates `std::env::vars()` and keeps entries whose name starts
    /// with `<PREFIX>_`. The `KEY` portion is the suffix after the
    /// prefix. Known non-boolean config keys (see
    /// [`Self::NON_FLAG_SUFFIXES`]) are skipped. Returns
    /// [`FlagError::InvalidValue`] on the first unparseable boolean
    /// value (fail-fast).
    pub fn from_env(prefix: &str) -> Result<Self, FlagError> {
        let needle = format!("{prefix}_");
        let mut flags = BTreeMap::new();
        for (k, v) in std::env::vars() {
            if let Some(suffix) = k.strip_prefix(&needle) {
                if suffix.is_empty() {
                    // `<PREFIX>_` with no key — skip silently.
                    continue;
                }
                if Self::NON_FLAG_SUFFIXES.contains(&suffix) {
                    continue;
                }
                let parsed = parse_bool(&v).ok_or_else(|| FlagError::InvalidValue {
                    var: k.clone(),
                    value: v.clone(),
                })?;
                flags.insert(suffix.to_string(), parsed);
            }
        }
        Ok(Self { flags })
    }

    /// `true` iff a flag named `<KEY>` is present and enabled.
    /// Unknown keys return `false` — the safe default.
    pub fn is_enabled(&self, key: &str) -> bool {
        self.flags.get(key).copied().unwrap_or(false)
    }

    /// Number of flags loaded.
    pub fn len(&self) -> usize {
        self.flags.len()
    }

    /// `true` iff no flags were loaded.
    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    /// Iterate over `(key, value)` pairs in alphabetical order
    /// (`BTreeMap` ordering).
    pub fn iter(&self) -> impl Iterator<Item = (&str, bool)> + '_ {
        self.flags.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// Parse a truthy/falsy string. Case-insensitive. `None` on
/// unrecognised input (caller turns this into `FlagError::InvalidValue`).
fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize env-mutating unit tests within this crate. Cross-crate
    /// races with `playcua-native` integration smoke are avoided by
    /// clearing *all* `<PREFIX>_*` vars for the duration of each test
    /// (not only the keys we set).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<F: FnOnce()>(prefix: &str, pairs: &[(&str, &str)], f: F) {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let needle = format!("{prefix}_");

        // Snapshot every matching var, then clear the whole prefix space.
        let mut saved: Vec<(String, String)> = Vec::new();
        for (k, v) in std::env::vars() {
            if k.starts_with(&needle) {
                saved.push((k, v));
            }
        }
        for (k, _) in &saved {
            std::env::remove_var(k);
        }
        for (k, v) in pairs {
            std::env::set_var(k, v);
        }
        f();
        // Remove what we set, then restore the prior snapshot.
        for (k, _) in pairs {
            std::env::remove_var(k);
        }
        for (k, v) in saved {
            std::env::set_var(k, v);
        }
    }

    #[test]
    fn non_flag_config_suffixes_are_skipped() {
        with_env(
            "PLAYCUA",
            &[
                ("PLAYCUA_MODALITY", "sandbox"),
                ("PLAYCUA_SANDBOX_BACKEND", "direct"),
                ("PLAYCUA_NVMS_CONFIG", "/tmp/nvms.toml"),
                ("PLAYCUA_VERBOSE", "1"),
            ],
            || {
                let flags = FlagSet::from_env("PLAYCUA").expect("config strings must not fail");
                assert!(flags.is_enabled("VERBOSE"));
                assert_eq!(flags.len(), 1);
            },
        );
    }

    #[test]
    fn from_env_reads_truthy_and_falsy() {
        with_env(
            "PLAYCUA",
            &[
                ("PLAYCUA_FOO_A", "1"),
                ("PLAYCUA_FOO_B", "true"),
                ("PLAYCUA_FOO_C", "YES"),
                ("PLAYCUA_FOO_D", "0"),
                ("PLAYCUA_FOO_E", "false"),
                ("PLAYCUA_FOO_F", "no"),
            ],
            || {
                let flags = FlagSet::from_env("PLAYCUA").expect("parse should succeed");
                assert!(flags.is_enabled("FOO_A"));
                assert!(flags.is_enabled("FOO_B"));
                assert!(flags.is_enabled("FOO_C"));
                assert!(!flags.is_enabled("FOO_D"));
                assert!(!flags.is_enabled("FOO_E"));
                assert!(!flags.is_enabled("FOO_F"));
            },
        );
    }

    #[test]
    fn unknown_key_defaults_to_false() {
        with_env("PLAYCUA", &[("PLAYCUA_KNOWN", "1")], || {
            let flags = FlagSet::from_env("PLAYCUA").unwrap();
            assert!(flags.is_enabled("KNOWN"));
            assert!(!flags.is_enabled("NEVER_SEEN"));
        });
    }

    #[test]
    fn unparseable_value_is_error() {
        with_env("PLAYCUA", &[("PLAYCUA_BAD", "not-a-bool")], || {
            let err = FlagSet::from_env("PLAYCUA").unwrap_err();
            assert!(matches!(err, FlagError::InvalidValue { .. }));
        });
    }

    #[test]
    fn len_and_iter_match_insertions() {
        with_env("PLAYCUA", &[("PLAYCUA_X", "1"), ("PLAYCUA_Y", "0")], || {
            let flags = FlagSet::from_env("PLAYCUA").unwrap();
            assert_eq!(flags.len(), 2);
            let pairs: Vec<_> = flags.iter().collect();
            assert_eq!(pairs, vec![("X", true), ("Y", false)]);
        });
    }
}
