//! Eidolon Core — Trait-based device automation abstraction.
//!
//! Defines unified interfaces for desktop, mobile, and sandbox automation.
//! All implementations are independent; no inter-crate dependencies.

pub mod error;
pub mod event;
pub mod input;
pub mod security;
pub mod stage_error;
pub mod stage_registry;
pub mod traits;
pub mod viewport;
pub mod virtual_stage;

pub use error::{AutomationResult, PhenoError, Result};
pub use event::{AutomationEvent, EventType, Platform};
pub use input::{InputType, PointerAction, PointerInput, TextInput};
pub use security::{
    validate_exec_cmd, validate_sandbox_id, NetworkPolicy, SandboxPolicy, EXEC_CMD_MAX_LEN,
    FORBIDDEN_EXEC_PATTERNS, SANDBOX_ID_MAX_LEN,
};
pub use stage_error::{StageError, StageResult};
pub use stage_registry::StageRegistry;
pub use traits::{DesktopAutomator, MobileAutomator, SandboxAutomator};
pub use viewport::Viewport;
pub use virtual_stage::{MobileStage, SandboxStage, VirtualStage, VirtualStageArcExt};

/// Eidolon version.
pub const VERSION: &str = "0.0.1";
