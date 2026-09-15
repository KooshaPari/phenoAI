# Functional Requirements — Eidolon (Phase 3 Layer)

> Phase 3 spec+test+traceability layer. Derived from codebase analysis of
> `crates/eidolon-core`, `crates/eidolon-desktop`, `crates/eidolon-mobile`,
> `crates/eidolon-sandbox`. ID format: `FR-EIDOLON-{NNN}`.

---

## FR-EIDOLON-001 — Unified Trait-Based Automation API

The system SHALL expose a single trait-based abstraction (`DesktopAutomator`,
`MobileAutomator`, `SandboxAutomator`) in `eidolon-core` that decouples callers
from platform implementations.

- **Source**: `crates/eidolon-core/src/traits.rs`
- **Rationale**: ADR-001 mandates trait-based design so any platform crate can be
  swapped without changing consumers.
- **Acceptance**: Trait objects (`Arc<dyn DesktopAutomator>`) compile and
  dispatch all methods without referencing concrete types.

---

## FR-EIDOLON-002 — Viewport Resolution and Orientation Detection

The system SHALL compute viewport orientation (`"landscape"` / `"portrait"`)
deterministically from width and height, and SHALL expose named presets for
common screen profiles (FHD desktop, FHD mobile, QHD tablet).

- **Source**: `crates/eidolon-core/src/viewport.rs`
- **Rationale**: Automation scripts must adapt input coordinates to orientation
  without hard-coding per-device constants.
- **Acceptance**: `Viewport::new(w, h, dpr).orientation` returns `"landscape"`
  when `w > h` and `"portrait"` otherwise. Presets `desktop_fhd()`,
  `mobile_fhd()`, `tablet_qhd()` exist and return correct dimensions.

---

## FR-EIDOLON-003 — Pointer and Text Input Serialisation

The system SHALL serialise and deserialise `PointerInput` and `TextInput`
values to/from JSON without data loss, enabling cross-process replay of
recorded automation events.

- **Source**: `crates/eidolon-core/src/input.rs`
- **Rationale**: Audit logs and replay pipelines require stable JSON
  representations of all input events.
- **Acceptance**: Round-trip `serde_json::to_string` → `from_str` preserves all
  fields (`x`, `y`, `button`, `action`, `duration_ms` for pointer;
  `text`, `input_type`, `delay_ms` for text).

---

## FR-EIDOLON-004 — Automation Event Audit Log

The system SHALL produce structured `AutomationEvent` records with a unique ID,
Unix timestamp, platform tag, and typed payload (`Pointer`, `Text`,
`Screenshot`, `Custom`) for every automation operation.

- **Source**: `crates/eidolon-core/src/event.rs`
- **Rationale**: HITL-less AI-DD workflows require a full audit trail of all
  device interactions for post-hoc review and training-data collection.
- **Acceptance**: `AutomationEvent::pointer`, `::text`, `::screenshot`
  constructors populate `id` (non-empty), `timestamp` (> 0), `platform`,
  and the correct `EventPayload` variant. Consecutive events have distinct IDs.

---

## FR-EIDOLON-005 — Sandbox Lifecycle Management

The system SHALL provide a `SandboxAutomator` implementation (`SandboxClient`)
that supports the full container/VM lifecycle: `get_metadata`, `start`, `stop`,
`exec`, `resource_usage`, and `record_event`. The implementation SHALL be
idempotent for `start` and `stop` invocations.

- **Source**: `crates/eidolon-sandbox/src/lib.rs`,
  `crates/eidolon-sandbox/src/docker/mod.rs`
- **Rationale**: Sandboxed test environments must handle repeated init/teardown
  calls from CI pipelines without error.
- **Acceptance**: Calling `start()` or `stop()` multiple times on the same
  client returns `Ok(())` each time. `exec()` returns a non-empty string.
  `resource_usage()` returns non-negative CPU and positive memory values.

---

## FR-EIDOLON-006 — Cross-Platform Send + Sync Safety

All trait implementations (`DesktopAutomator`, `MobileAutomator`,
`SandboxAutomator`) SHALL satisfy the `Send + Sync` auto-traits so they can
be shared across async task boundaries via `Arc<dyn Trait>`.

- **Source**: `crates/eidolon-core/src/traits.rs` (trait bounds),
  implementation crates
- **Rationale**: Concurrent automation workflows (e.g. parallel device farms)
  require shared handles across Tokio tasks.
- **Acceptance**: `fn assert_send_sync<T: Send + Sync>()` compiles for every
  concrete type in each implementation crate.

---

## FR-EIDOLON-007 — Version Constant Exposure

The system SHALL expose a `VERSION` constant from `eidolon-core` that matches
the `Cargo.toml` package version, enabling runtime version reporting.

- **Source**: `crates/eidolon-core/src/lib.rs`
- **Rationale**: Observability tooling and release pipelines must be able to
  embed and query the library version at runtime without shelling out.
- **Acceptance**: `eidolon_core::VERSION` equals the `version` field in
  `crates/eidolon-core/Cargo.toml`.
