# KDesktopVirt → Eidolon Consolidation Plan

**Status:** Accepted — forward-only migration. KDesktopVirt is deprecated as a
standalone repo. Eidolon (`crates/eidolon-sandbox/` and `crates/eidolon-desktop/`)
is the canonical destination.

**Date:** 2026-06-30
**Author:** v37 audit + operator directive 2026-06-30
**Related ADR:** `docs/adr/ADR-002-virtual-stage-unification.md`
**Related extraction plan:** `docs/EXTRACTION_PLAN.md` (Phase 2: KDesktopVirt FFmpeg + security)

---

## Why consolidate

KDesktopVirt v37 audit mean: **1.09 / 3** (factory level 1/4).
Root causes that make standalone maintenance unsustainable:

1. Identity drift — package name `kvirtualstage`, repo URL `KVirtualStage`, VERSION
   `0.1.0` vs Cargo.toml `0.2.1` (fixed in KDesktopVirt PR #64, but a symptom of
   org-level fragmentation).
2. Eidolon already owns `VirtualStage` (ADR-002) and `PlayCuaDispatcher` — two of
   KDesktopVirt's core abstractions are already present in canonical form.
3. KDesktopVirt's security architecture (YAML docs + code reality gap), placeholder
   metrics, and no-op CI gates cannot be fixed without duplicating work Eidolon has
   already done (`phenotype-error-core`, `eidolon-core::security`, `SandboxPolicy`).
4. The operator consolidation directive (2026-06-30): "forward-only: extract →
   Eidolon, deprecate KDesktopVirt."

---

## Module mapping: KDesktopVirt → Eidolon destination

The table below is the authoritative mapping. Each row states the source module,
what is worth porting, the Eidolon destination, and the migration risk.

| KDesktopVirt module | Worth porting | Eidolon destination | Risk / notes |
|---|---|---|---|
| `src/containerization.rs` | Container lifecycle helpers (`ContainerizationEngine`, `ContainerConfig`, `ContainerHandle`) | `crates/eidolon-sandbox/src/container.rs` (new) | Medium — depends on `bollard`; add as optional dep behind `sandbox-docker` feature |
| `src/session_storage.rs` | Session persistence types and TTL logic | `crates/eidolon-sandbox/src/session.rs` (new) | Low — pure data + `redis` optional dep; replaces placeholder in `playcua_dispatcher` |
| `src/metrics.rs` (real sysinfo impl after PR #64) | `MetricsCollector`, Prometheus export, structured log event | `crates/eidolon-sandbox/src/metrics.rs` (new) | Low — self-contained; gate on `sandbox-metrics` feature; depends on `sysinfo`, `hostname` |
| `src/security_framework.rs` | Capability allow-list + rate limit only | `eidolon-desktop::security_hooks` (+ core for resource/validators) | ✅ Phase F — vault/OAuth/engine **not** ported |
| `src/audit_compliance.rs` | `AuditEngine`, `ComplianceReport`, retention + integrity types | `crates/eidolon-sandbox/src/audit.rs` (new) | Medium — large surface; strip the SIEM/GDPR strings that aren't enforced |
| `src/recording_pipeline.rs` + `src/ffmpeg_pipeline.rs` | `RecordingPipeline`, `QualityProfile`, FFmpeg dispatch | `crates/eidolon-desktop/src/recording/` | ✅ Phase E — system `ffmpeg` behind `desktop-recording`; DO NOT port `ffmpeg_pipeline_broken.rs`; no RTMP/audio DSP/bollard |
| `src/automation_engine.rs` | `WindMouseEngine`, `NaturalTypingEngine`, `AutomationWorkflow` | `crates/eidolon-desktop/src/automation.rs` (new) | Medium — platform-agnostic; test coverage is good (20+ unit tests) |
| `src/desktop_control.rs` | `DesktopControlManager` | `eidolon-desktop` `windows` / `linux` / `macos` | **Windows SendInput+DXGI/GDI landed** (`desktop-windows`, 2026-07-21); **Linux X11 XTEST+GetImage landed** (`desktop-linux`, 2026-07-21); **pure Wayland portal Screenshot + RemoteDesktop/Screencast inject landed** (`ashpd`, 2026-07-22); macOS already real |
| `src/ui_automation.rs` | `UiAutomationEngine`, `WindMouseConfig`, accessibility types | `crates/eidolon-desktop/src/ui_automation.rs` (new) | Low — well-tested (8 unit tests pass); port with tests |
| `src/virtualization.rs` | Container + VM provisioning traits | Evaluate against `eidolon-sandbox::docker` — likely redundant | High — `eidolon-sandbox::docker` is the canonical location; KDesktopVirt's version is broader but less tested |
| `src/audio*.rs` / `src/tts_audio_system.rs` | Audio/TTS pipeline | **Do not port in this pass.** No Eidolon consumer yet; STT/TTS is out of scope for desktop automation core. | Deferred |
| `src/api_surface.rs` (C FFI / PyO3 bindings) | `unsafe extern "C"` FFI surface | **Do not port directly.** Eidolon's FFI strategy will use `uniffi` / `PyO3` via a dedicated binding crate (`eidolon-ffi`). Map the port traits, not the FFI wrappers. | Deferred to `eidolon-ffi` crate |
| `src/mcp.rs` | MCP server integration | **Do not port.** MCP integration lives in the consuming agent, not in the automation library. | Out of scope |
| `src/api.rs` / `src/web.rs` | HTTP API server | **Do not port.** Eidolon is a library, not a server. Routing/web belongs in `kvs-server` consumer. | Out of scope |
| `src/natural_automation_demo.rs` / `src/animation_framework.rs` | Demo / animation helpers | **Do not port.** Commented out upstream; not suitable for a library surface. | Skip |
| Python scripts (`*.py` in repo root) | None — all are demo/validation scripts | **Do not port.** | Skip |

---

## Execution order (dependency DAG)

Migration must proceed in this order to keep Eidolon green at each step:

```
Phase A — No new deps (can land in one PR)
  A1. Port ui_automation.rs → eidolon-desktop/src/ui_automation.rs
      + automation_engine.rs (WindMouseEngine, NaturalTypingEngine) → eidolon-desktop/src/automation.rs
      Prerequisite: none. Both have existing unit tests.

Phase B — sysinfo dep (one PR)
  B1. Port metrics.rs (post-PR#64 sysinfo version) → eidolon-sandbox/src/metrics.rs
      Behind feature flag `sandbox-metrics`.
      Prerequisite: A1 (establishes the pattern for feature-gated modules).

Phase C — bollard dep (one PR)
  C1. Port containerization.rs → eidolon-sandbox/src/container.rs
      Behind feature flag `sandbox-docker`.
      Prerequisite: B1.
  **A+ T2 (2026-07-20) live slice (no unarchive):** `BollardDockerOrchestrator` +
  `DockerSandboxClient` + `docker::probe` in `eidolon-sandbox/src/docker/`.
  Fail-loud stub retained when daemon absent. See `docs/EXTRACTION_PLAN.md` Phase C.

Phase D — session + audit (one PR)
  D1. Port session_storage.rs → eidolon-sandbox/src/session.rs
  D2. Port audit_compliance.rs → eidolon-sandbox/src/audit.rs
  Prerequisite: C1.
  **A+ Phase D (2026-07-20) complete (no unarchive):** hexagonal
  `SessionStore` / `AuditEngine` with memory + fail-loud unavailable;
  `FileSessionStore` / `FileAuditStore` behind `sandbox-session` /
  `sandbox-audit`. Honest compliance reports (no SIEM/GDPR theatre).
  **Redis (2026-07-21):** `RedisSessionStore` behind `sandbox-session-redis`
  (wraps `redis` 1.4; explicit URL + PING; no fake pool theatre).
  **Composition (2026-07-21):** `AuditingSandbox` +
  `SandboxClient::with_audit` / `with_memory_audit` route `record_event`
  into `AuditEngine` (unavailable store fail-loud).
  **Audit indexes (2026-07-22):** secondary query indexes (time, event type,
  actor, target, correlation) + persisted `*.jsonl.idx.json` sidecar; corrupt
  index → `EIDOLON_SANDBOX_AUDIT_INDEX` (no silent degradation).
  See `docs/EXTRACTION_PLAN.md` Phase D.

Phase E — recording (separate PR, high-risk)
  E1. Port recording_pipeline.rs + ffmpeg_pipeline.rs → eidolon-desktop/src/recording.rs
      Requires ffmpeg system binary; gate on `desktop-recording` feature.
      Add integration test gated on `RECORDING_INTEGRATION=1`.
  Prerequisite: D1.
  **A+ Phase E (2026-07-20) complete (no unarchive):** `DesktopRecorder` +
  `QualityProfile` / `VideoFormat` + `recording::probe` + feature-gated
  `FfmpegRecorder` / `RecordingPipeline` (capture start/stop, file encode,
  GIF encode). Fail-loud when system `ffmpeg` missing. Live capture gated on
  `RECORDING_INTEGRATION=1`. Intentionally **not** ported: RTMP/WebRTC
  streaming, audio DSP, bollard container recording, `ffmpeg_pipeline_broken`.

Phase F — security overlap resolution (separate PR)
  F1. Audit security_framework.rs against eidolon-core::security.
      Port non-overlapping types only; delete duplicates.
  Prerequisite: E1.
  **A+ Phase F (2026-07-20) complete (no unarchive):** audited via GitHub
  contents API. Deduped `ResourceLimits` → `SandboxPolicy`/`NetworkPolicy`;
  wired `DesktopSecurityGate` → core validators; ported allow-list + rate
  limit as `PolicySecurityGate`; vault/OAuth/engine fail-loud
  (`EIDOLON_DESKTOP_SECURITY_UNAVAILABLE`). AuditLogger → Phase D ✅.

Phase G — FFI (future, separate crate)
  G1. Create `crates/eidolon-ffi` crate.
      Re-expose VirtualStage surface via uniffi / PyO3.
      Do NOT copy KDesktopVirt's unsafe C FFI wrappers directly.
  Prerequisite: F1.
```

---

## What Eidolon already has (do not duplicate)

| KDesktopVirt capability | Eidolon equivalent | Status |
|---|---|---|
| `VirtualStage` unified trait surface | `eidolon-core::virtual_stage::VirtualStage` | Fully implemented, 10+ tests |
| `SandboxAutomator` dispatch | `eidolon-sandbox::playcua_dispatcher::PlayCuaDispatcher` | Fully implemented, 8 tests |
| `SandboxClient` with input validation | `eidolon-sandbox::SandboxClient` | Fully implemented |
| `validate_sandbox_id`, `validate_exec_cmd` | `eidolon-core::security` | Fully implemented; desktop hooks delegate |
| `SandboxPolicy` (resource limits) | `eidolon-core::security::SandboxPolicy` | Fully implemented (dedupes KDesktopVirt `ResourceLimits`) |
| Capability allow-list / rate limit | `eidolon-desktop::PolicySecurityGate` | Phase F complete |
| Vault / OAuth / AES / Argon2 | — | Intentionally not ported (fail-loud) |
| `PhenoError` / `StageError` error envelope | `eidolon-core::error`, `eidolon-core::stage_error` | Fully implemented |
| Docker orchestration | `eidolon-sandbox::docker` | **T2 live** bollard behind `sandbox-docker`; stub when daemon absent |

---

## KDesktopVirt tombstone requirement

Before Phase A begins, a tombstone PR must land on KDesktopVirt main:

1. Update `README.md` header with deprecation notice pointing to Eidolon.
2. Update `Cargo.toml` `[package]` with `readme` note.
3. Close or lock the repo (operator action — not automatable here).

The tombstone PR is separate from this consolidation plan and is tracked in
`KDesktopVirt` PR #65 (or equivalent).

---

## Acceptance criteria for "consolidation complete"

- [ ] Phases A–D land in Eidolon and all 168+ existing tests still pass.
- [ ] Each ported module has at least the same unit-test coverage as the KDesktopVirt
      original (verified by `cargo test`).
- [ ] KDesktopVirt tombstone PR merged on that repo.
- [ ] `docs/EXTRACTION_PLAN.md` Phase 2 section updated to "complete".
- [ ] `ADR-002-virtual-stage-unification.md` references updated to note KDesktopVirt
      as absorbed.
- [x] Phase D (session + audit) complete in Eidolon (2026-07-20) without unarchive.
- [x] Phase F (security overlap) complete in Eidolon (2026-07-20) without unarchive.
- [x] Phase E recording pipeline complete in Eidolon (2026-07-20) without unarchive
      (system `ffmpeg`; no RTMP/audio DSP/bollard; no `ffmpeg_pipeline_broken`).
- [ ] Phase G (`eidolon-ffi`) tracked as a separate issue with a `uniffi` ADR.
