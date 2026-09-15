//! Modality registry + selection.
//!
//! Provides:
//! - `ModalityRegistry`: holds a `Vec<Box<dyn Modality>>` and the current
//!   `SelectedModality` (the result of the most recent select call).
//! - `ModalityEnv`: the inputs to selection (CLI flag, env var, "auto").
//! - `SelectedModality`: the outcome of selection — a cloneable handle that
//!   can be passed into the App / Dispatcher for observability.

use super::{Modality, ModalityKind};
use std::sync::Arc;

/// Inputs to modality selection, in the order they are honored.
#[derive(Debug, Clone, Default)]
pub struct ModalityEnv {
    /// `--modality` CLI flag (highest precedence).
    pub flag: Option<ModalityKind>,
    /// `PLAYCUA_MODALITY` env var (or `PLAYCUA_MODALITY` parsed by the caller).
    pub env: Option<ModalityKind>,
    /// If true, the selector falls back to a host-OS heuristic.
    pub auto: bool,
}

impl ModalityEnv {
    /// Build a ModalityEnv from the current process argv + env.
    ///
    /// Recognized sources:
    /// - `PLAYCUA_MODALITY` env var (lowercase; "auto" enables heuristic)
    /// - caller-supplied `flag` from CLI parsing
    pub fn from_process_env(flag: Option<ModalityKind>) -> Self {
        let env_str = std::env::var("PLAYCUA_MODALITY").ok();
        let (env, auto) = match env_str.as_deref() {
            Some("auto") => (None, true),
            Some(other) => (ModalityKind::parse(other), false),
            None => (None, false),
        };
        Self { flag, env, auto }
    }
}

/// The result of a successful modality selection.
#[derive(Debug, Clone)]
pub struct SelectedModality {
    pub kind: ModalityKind,
    /// Human-readable description (e.g. "xcap/enigo on macOS 14.4").
    pub describe: &'static str,
    /// Extra detail for logs (e.g. "nvms=/usr/local/bin/nvms").
    pub detail: String,
    /// True if the modality's `is_available()` probe passed.
    pub available: bool,
}

/// Registry of all known modalities + the result of the most recent
/// selection. Cheap to clone (`Arc` inside, but the Vec of dyn Modality
/// is owned; clone is `O(n)`).
pub struct ModalityRegistry {
    modalities: Vec<Box<dyn Modality>>,
    selected: Option<SelectedModality>,
}

impl ModalityRegistry {
    /// Build a registry with all five modalities, in selection-precedence order.
    pub fn with_defaults() -> Self {
        use super::container::ContainerModality;
        use super::native::NativeModality;
        use super::nvms::NvmsModality;
        use super::sandbox::SandboxModality;
        use super::wsl::WslModality;

        let modalities: Vec<Box<dyn Modality>> = vec![
            Box::new(SandboxModality::new()),
            Box::new(NvmsModality::new()),
            Box::new(WslModality::new()),
            Box::new(ContainerModality::new()),
            Box::new(NativeModality::new()),
        ];
        Self {
            modalities,
            selected: None,
        }
    }

    /// Build a registry with only the given modalities (in order).
    /// Used by tests; production callers should use `with_defaults()`.
    pub fn custom(modalities: Vec<Box<dyn Modality>>) -> Self {
        Self {
            modalities,
            selected: None,
        }
    }

    /// Iterate over all registered modalities.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Modality> {
        self.modalities.iter().map(|b| b.as_ref())
    }

    /// Find a modality by kind.
    pub fn find(&self, kind: ModalityKind) -> Option<&dyn Modality> {
        self.modalities
            .iter()
            .find(|m| m.kind() == kind)
            .map(|b| b.as_ref())
    }

    /// Run selection. Returns the selected modality's SelectedModality
    /// (also stored on the registry for later introspection).
    pub fn select(&mut self, env: &ModalityEnv) -> &SelectedModality {
        let chosen_kind = self.resolve(env);
        let chosen = self
            .find(chosen_kind)
            .expect("NativeModality is always registered; this is unreachable");

        self.selected = Some(SelectedModality {
            kind: chosen.kind(),
            describe: chosen.describe(),
            detail: chosen.detail(),
            available: chosen.is_available(),
        });
        self.selected.as_ref().unwrap()
    }

    /// Last selection result, if any.
    pub fn selected(&self) -> Option<&SelectedModality> {
        self.selected.as_ref()
    }

    /// Resolve a ModalityEnv to a concrete ModalityKind.
    /// Selection precedence: flag > env > auto > native.
    fn resolve(&self, env: &ModalityEnv) -> ModalityKind {
        // 1. Explicit flag wins.
        if let Some(k) = env.flag {
            return k;
        }
        // 2. Explicit env var wins (over auto).
        if let Some(k) = env.env {
            return k;
        }
        // 3. Auto: pick the first available, in registry order.
        if env.auto {
            for m in self.iter() {
                if m.is_available() {
                    return m.kind();
                }
            }
        }
        // 4. Fallback: native.
        ModalityKind::Native
    }
}

/// Tiny wrapper so the registry can be cheaply shared between tasks.
#[derive(Clone)]
pub struct SharedModalityRegistry(Arc<std::sync::Mutex<ModalityRegistry>>);

impl SharedModalityRegistry {
    pub fn new(reg: ModalityRegistry) -> Self {
        Self(Arc::new(std::sync::Mutex::new(reg)))
    }

    pub fn select(&self, env: &ModalityEnv) -> SelectedModality {
        let mut g = self.0.lock().expect("modality registry poisoned");
        g.select(env).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: a modality that always reports available.
    struct AlwaysAvail {
        kind: ModalityKind,
        detail: &'static str,
    }
    impl Modality for AlwaysAvail {
        fn kind(&self) -> ModalityKind {
            self.kind
        }
        fn describe(&self) -> &'static str {
            "test-always"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn detail(&self) -> String {
            self.detail.to_string()
        }
    }

    /// Test helper: a modality that is never available.
    struct NeverAvail(ModalityKind);
    impl Modality for NeverAvail {
        fn kind(&self) -> ModalityKind {
            self.0
        }
        fn describe(&self) -> &'static str {
            "test-never"
        }
        fn is_available(&self) -> bool {
            false
        }
    }

    fn env(flag: Option<ModalityKind>, var: Option<ModalityKind>, auto: bool) -> ModalityEnv {
        ModalityEnv {
            flag,
            env: var,
            auto,
        }
    }

    #[test]
    fn flag_wins_over_env() {
        let mut reg = ModalityRegistry::custom(vec![
            Box::new(AlwaysAvail {
                kind: ModalityKind::Nvms,
                detail: "",
            }),
            Box::new(AlwaysAvail {
                kind: ModalityKind::Native,
                detail: "",
            }),
        ]);
        let s = reg.select(&env(
            Some(ModalityKind::Native),
            Some(ModalityKind::Nvms),
            false,
        ));
        assert_eq!(s.kind, ModalityKind::Native);
    }

    #[test]
    fn env_wins_over_auto() {
        let mut reg = ModalityRegistry::custom(vec![
            Box::new(AlwaysAvail {
                kind: ModalityKind::Nvms,
                detail: "",
            }),
            Box::new(AlwaysAvail {
                kind: ModalityKind::Native,
                detail: "",
            }),
        ]);
        let s = reg.select(&env(None, Some(ModalityKind::Native), true));
        assert_eq!(s.kind, ModalityKind::Native);
    }

    #[test]
    fn auto_picks_first_available() {
        let mut reg = ModalityRegistry::custom(vec![
            Box::new(NeverAvail(ModalityKind::Nvms)),
            Box::new(AlwaysAvail {
                kind: ModalityKind::Container,
                detail: "",
            }),
        ]);
        let s = reg.select(&env(None, None, true));
        assert_eq!(s.kind, ModalityKind::Container);
    }

    #[test]
    fn auto_falls_back_to_native_when_nothing_available() {
        let mut reg = ModalityRegistry::custom(vec![
            Box::new(NeverAvail(ModalityKind::Nvms)),
            Box::new(AlwaysAvail {
                kind: ModalityKind::Native,
                detail: "",
            }),
        ]);
        let s = reg.select(&env(None, None, true));
        assert_eq!(s.kind, ModalityKind::Native);
    }

    #[test]
    fn no_flag_no_env_no_auto_picks_native() {
        let mut reg = ModalityRegistry::custom(vec![
            Box::new(AlwaysAvail {
                kind: ModalityKind::Nvms,
                detail: "",
            }),
            Box::new(AlwaysAvail {
                kind: ModalityKind::Native,
                detail: "",
            }),
        ]);
        let s = reg.select(&env(None, None, false));
        // Default fallback: native (always last, always wins when nothing else chosen).
        assert_eq!(s.kind, ModalityKind::Native);
    }

    #[test]
    fn selected_modality_preserves_describe_and_detail() {
        let mut reg = ModalityRegistry::custom(vec![Box::new(AlwaysAvail {
            kind: ModalityKind::Native,
            detail: "host=macos",
        })]);
        let s = reg.select(&env(Some(ModalityKind::Native), None, false));
        assert_eq!(s.describe, "test-always");
        assert_eq!(s.detail, "host=macos");
        assert!(s.available);
    }
}
