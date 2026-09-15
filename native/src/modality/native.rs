//! `Native` modality — drive the host OS directly using platform-native APIs.
//!
//! This is the only modality with a fully-wired App in this slice. It is
//! always available (the only modality for which that is true).
//!
//! ## Selection reasoning
//!
//! `Native` is selected when:
//! 1. `--modality native` is set explicitly, OR
//! 2. `PLAYCUA_MODALITY=native` is set, OR
//! 3. `auto` is selected and no other modality passes `is_available()`.

use super::{Modality, ModalityKind};

/// The native-modality probe. Stateless; safe to construct once at startup.
pub struct NativeModality;

impl NativeModality {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeModality {
    fn default() -> Self {
        Self::new()
    }
}

impl Modality for NativeModality {
    fn kind(&self) -> ModalityKind {
        ModalityKind::Native
    }

    fn describe(&self) -> &'static str {
        "drive the host OS directly (xcap/enigo/CGEvent/uinput/SendInput)"
    }

    fn is_available(&self) -> bool {
        // Native is always available. If we can construct a process, we can
        // drive the host.
        true
    }

    fn detail(&self) -> String {
        format!("host={}", std::env::consts::OS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_is_always_available() {
        let m = NativeModality::new();
        assert_eq!(m.kind(), ModalityKind::Native);
        assert!(m.is_available());
        assert!(m.detail().starts_with("host="));
    }
}
