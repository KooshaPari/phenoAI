# Functional Requirements — Eidolon

Traces to: PRD.md epics E1–E7.
ID format: FR-EIDOLON-{NNN}.

---

## Unified Device Automation API

**FR-EIDOLON-001**: The system SHALL provide a trait-based abstraction for device automation spanning desktop, mobile, and virtual environments.
Traces to: E1.1

**FR-EIDOLON-002**: The system SHALL implement [Device] trait for macOS, Linux, Windows, iOS, and Android with standard methods: click, type, screenshot, wait_for_element.
Traces to: E1.2

**FR-EIDOLON-003**: All device implementations SHALL support element selection via accessibility tree, coordinates, and image recognition (OCR/template matching).
Traces to: E1.3

---

## Sandboxed Environment Support

**FR-EIDOLON-004**: The system SHALL support automation of sandboxed environments (Docker, VirtualBox, NanoVMs) via standardized control APIs.
Traces to: E2.1

**FR-EIDOLON-005**: The system SHALL provide a [SandboxDevice] adapter that converts control commands to container/VM-specific invocations.
Traces to: E2.2

---

## Session Management

**FR-EIDOLON-006**: The system SHALL maintain device sessions with lifecycle management (init, connect, authenticate, disconnect, cleanup).
Traces to: E3.1

**FR-EIDOLON-007**: The system SHALL support session isolation to enable concurrent automation workflows on the same device.
Traces to: E3.2

---

## Trace & Test Guidance

All tests MUST reference a Functional Requirement (FR):

```rust
// Traces to: FR-EIDOLON-NNN
#[test]
fn test_device_click_element() { ... }
```
