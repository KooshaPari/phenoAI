# Extraction Plan — Eidolon

Per KDesktopVirt audit: design fresh with trait-based core. Selectively extract salvageable modules from sibling projects.

## Archive policy (A+ fleet decision)

| Repo | GitHub | Policy |
|------|--------|--------|
| kmobile | KooshaPari/kmobile | **Remain archived** — extract into `eidolon-mobile` on demand |
| mobile-cli | KooshaPari/mobile-cli | **Remain archived** — extract adapters only |
| mobile-mcp | KooshaPari/mobile-mcp | **Remain archived** — MCP surface may be re-homed later |
| KDesktopVirt | KooshaPari/KDesktopVirt | **Remain archived** — extract FFmpeg/security into `eidolon-desktop` / core |
| PlayCua | KooshaPari/PlayCua | **Active** — not a satellite; sandbox patterns live here |
| bare-cua | (deprecated) | Frozen — see PlayCua `DEPRECATED_BARE_CUA.md` |

Do **not** unarchive satellites for routine work. Unarchive only for a scoped extraction PR that copies salvageable modules into Eidolon, then leave the satellite archived.

Canonical roster also lives in the README [Source / archived satellites](../README.md#source--archived-satellites) table.

## Source Projects

### KDesktopVirt (macOS/Windows/Linux automation)

**Status**: Core infrastructure broken; FFmpeg pipeline + security framework salvageable.

| Module | Status | Target in Eidolon | Notes |
|--------|--------|-------------------|-------|
| FFmpeg screenshot pipeline | ✅ Working | `eidolon-desktop` | Handles encoding, performance profiling |
| Security framework (sandboxing) | ✅ Working | `eidolon-core` error/permissions layer | Rate limiting, capability checks |
| Pointer/keyboard input system | ❌ Broken | Design fresh | XCTest/UiAutomator pattern preferred |
| Native API bindings (macOS NSScreen, etc.) | ⚠️ Partial | `eidolon-desktop` | Salvage working bits; modernize |
| tts_audio_system | ❌ Broken | Do NOT extract | Replace with external TTS service |
| ffmpeg_pipeline_broken | ❌ Broken | Do NOT extract | Use working FFmpeg pipeline instead |

### kmobile (iOS/Android automation)

**Status**: Archived kmobile remains reference-only. Eidolon now ships **fresh CLI-driven** adapters (no satellite unarchive) behind `mobile-ios` / `mobile-android`.

| Module | Status in Eidolon | Target path | Notes |
|--------|-------------------|-------------|-------|
| XCTest adapter | 🟨 T1 CLI + XCUI bridge + in-tree XCUITest + Rust fallback (`mobile-ios` / `mobile-xcui-helper`) | `eidolon-mobile/src/ios/` + `native/ios/EidolonXcuiHelper` | `XcrunIosDriver`: hermetic `xcrun` + `simctl` list + gated `simctl` screenshot; tap/swipe/text/viewport via `XcuiBridge` (`EIDOLON_IOS_XCUI_BUNDLE` wins, else built `eidolon-xcui-xctest`, else Rust `eidolon-xcui-helper`, else AppleScript); codes `EIDOLON_MOBILE_IOS_STUB` / `_ACTIONS_GATED` / `_IOS_XCUI_UNAVAILABLE` |
| UiAutomator adapter | 🟨 T1 CLI + UIA2 lifecycle + Appium-compatible HTTP session (`mobile-android` / `mobile-uia2` / `mobile-appium`) | `eidolon-mobile/src/android/` + `native` | `AdbAndroidDriver`: hermetic `adb` + devices; **real** gated `input` + screencap; `Uia2Server` Appium-shaped install/start/stop (env → SHA-256 assets/cache / `eidolon-fetch-uia2`); `Uia2HttpClient` / `AppiumSessionClient` (ureq) W3C/Appium verbs on `:6790` / `:4723`; optional `appium_probe`; code `_UIA2_UNAVAILABLE` / `_APPIUM_UNAVAILABLE` |
| Device discovery | 🟨 T1 CLI | `eidolon-mobile/src/discovery.rs` | `InstrumentDiscovery` when features on; else `EIDOLON_MOBILE_DISCOVERY_UNAVAILABLE`; in-memory `DeviceManager` for tests |
| Screenshot capture | 🟨 T1 gated | `MobileAutomator::screenshot` | iOS: `simctl io` when gated + UDID; Android: `adb exec-out screencap` when gated |

**A+ T1 mobile slice (2026-07-21) — CLI drivers + actions + UIA2 hooks, no satellite unarchive:**

| Item | Status |
|------|--------|
| Documented codes (`EIDOLON_MOBILE_IOS_STUB`, `_ANDROID_STUB`, `_OTHER_STUB`, `_DISCOVERY_UNAVAILABLE`, `_ACTIONS_GATED`, `_IOS_XCUI_UNAVAILABLE`, `_UIA2_UNAVAILABLE`, `_APPIUM_UNAVAILABLE`) | ✅ `eidolon-mobile::codes` |
| `ios` / `android` trait hooks + fail-loud stubs | ✅ retained for feature-off / tools-absent |
| Feature `mobile-ios` → `XcrunIosDriver` | ✅ hermetic `xcrun` / `simctl` list + gated screenshot; `XcuiBridge` for tap/swipe/text/viewport |
| `XcuiBridge` (`BundleXcuiBridge` / AppleScript / `MissingXcuiBridge`) | ✅ env override wins; else prefer built XCUITest `eidolon-xcui-xctest`; else Rust `eidolon-xcui-helper`; else `EIDOLON_IOS_ALLOW_APPLESCRIPT=1` |
| Feature `mobile-xcui-helper` → bin `eidolon-xcui-helper` | ✅ AppleScript/simctl **fallback**; fail-loud without Booted sim / non-macOS |
| In-tree XCUITest `.xcodeproj` (`native/ios/EidolonXcuiHelper`) | ✅ host + UITests + argv runner; build via `Scripts/build-for-testing.swift` → `./build` (not `/tmp`); live gate `EIDOLON_XCUI_XCODE_INTEGRATION=1` |
| Feature `mobile-android` → `AdbAndroidDriver` | ✅ hermetic `adb` / devices; **real** gated `input` + screencap |
| Feature `mobile-uia2` → `Uia2Server` + `Uia2HttpClient` / `AppiumSessionClient` | ✅ Appium-shaped probe/install/forward/start/stop via adb; pinned Appium APKs via env → assets → durable SHA-256 cache (`eidolon-fetch-uia2` / `ensure`); **Appium-compatible** HTTP session client (ureq → findElement(s)/click/clear/sendKeys/text/source/windows/contexts/actions/screenshot/timeouts) when `uia2_server_ready`; gated session/click/actions |
| Feature `mobile-appium` → `appium_probe` | ✅ optional `appium` CLI / `APPIUM_HOME` / `EIDOLON_APPIUM_URL` discovery + `GET /status`; hermetic when absent; code `_APPIUM_UNAVAILABLE` when required |
| `discovery` → `InstrumentDiscovery` (features) | ✅ live list when CLIs present; empty `[]` ok |
| `MobileClient` → `UnsupportedPlatform` (501) | ✅ fail-loud default (use feature drivers for live) |
| Appium Desktop GUI | ✅ **partial** — `mobile-appium-desktop`: Appium Inspector launcher + local HTML dashboard (`eidolon-appium-desktop`); session HTTP via `mobile-uia2`/`mobile-appium`; not a full Electron Appium Desktop fork; do not unarchive kmobile routinely |

### KVirtualStage (container/VM automation)

**Status**: VM orchestration patterns. **A+ T2** lands live Docker (`sandbox-docker`), nanoVM CLI (`sandbox-nanovm`), KVM/Firecracker CLI (`sandbox-kvm`), shared unikernel/rootfs scaffolding (`sandbox-unikernel`), hermetic + env-gated live rootfs packaging (`sandbox-rootfs-pack` / `ROOTFS_PACK_INTEGRATION=1`) including **vsock-agent bake** into rootfs trees (`pack::bake_agent` / `bake_agent_then_pack`), **canned rootfs pipeline** (`pack::canned` / `eidolon-canned-rootfs` → durable `target/canned-rootfs/`), **GH Ext4 release staging + pin/fetch** (`pack::release_asset` / `eidolon-release-rootfs` / `eidolon-fetch-rootfs`; default pin filled for `rootfs-v0.1.0`), env-gated plan→guest process spawn via `unikernel::boot`, **serial/stdio guest exec** via `unikernel::exec` (`UNIKERNEL_EXEC_INTEGRATION=1`), **vsock NDJSON + Linux AF_VSOCK** via `unikernel::vsock` (`sandbox-vsock` + `UNIKERNEL_VSOCK_INTEGRATION=1`), and **in-guest vsock agent** via `unikernel::vsock_agent` + bin `eidolon-vsock-agent` (`sandbox-vsock-agent`). Pack bake + canned + release pipeline shipped; actual GH asset publish remains a manual one-shot.

| Module | Status | Target in Eidolon | Notes |
|--------|--------|-------------------|-------|
| Docker adapter | ✅ T2 live (`sandbox-docker`) | `eidolon-sandbox/src/docker/` | bollard start/stop/exec; daemon absent → `EIDOLON_SANDBOX_DOCKER_STUB` |
| Resource monitoring | ✅ T2 | `BollardDockerOrchestrator::get_resource_usage` | one-shot stats + Moby CPU % from `cpu_stats`/`precpu_stats` deltas (`docker::stats`); `0.0` when deltas absent |
| nanoVMs integration | 🟨 T2 CLI + plan boot + serial exec (`sandbox-nanovm`) | `eidolon-sandbox/src/nanovm/` | `ops` probe + hermetic start; `try_from_plan` + `NANOVM_INTEGRATION=1` spawn; serial exec + `UNIKERNEL_EXEC_INTEGRATION=1` |
| KVM / Firecracker | 🟨 T2 CLI + plan boot + serial exec (`sandbox-kvm`) | `eidolon-sandbox/src/kvm/` | `firecracker` probe + hermetic start; `try_from_plan` + `FIRECRACKER_INTEGRATION=1` spawn; serial exec |
| Unikernel / rootfs | 🟨 T2 scaffold + boot + serial/vsock exec + pack bake + canned + release staging (`sandbox-unikernel` / `sandbox-vsock`) | `eidolon-sandbox/src/unikernel/` | config + probe + `boot`/`exec`/`vsock`/`pack::bake_agent`/`pack::canned`/`pack::release_asset`; env-gated spawn/exec; default GH pin filled (`rootfs-v0.1.0`) |
| In-guest vsock agent | ✅ T2 bin (`sandbox-vsock-agent`) | `unikernel::vsock_agent` + `eidolon-vsock-agent` | NDJSON v1; Linux listen; macOS fail-loud; pack bake + guide `docs/guides/vsock-guest-agent.md` |
| Rootfs image pack | ✅ T2 hermetic + live + agent bake + canned + release (`sandbox-rootfs-pack`) | `eidolon-sandbox/src/unikernel/pack/` | manifest + stage + bake + `canned` + `release_asset`; live mkfs/virt-make-fs/docker/DockerToExt4 when `ROOTFS_PACK_INTEGRATION=1` |

**A+ T2 sandbox slice (2026-07-20) — Docker + nanoVM/KVM CLI, no KDesktopVirt unarchive:**

| Item | Status |
|------|--------|
| Documented codes (`EIDOLON_SANDBOX_DOCKER_STUB`, `_NANOVM_STUB`, `_KVM_STUB`, `_OTHER_STUB`) | ✅ `eidolon-sandbox::codes` |
| `docker` / `nanovm` / `kvm` trait hooks + fail-loud stubs | ✅ stub path retained for CI / tools-absent |
| Feature `sandbox-docker` → `BollardDockerOrchestrator` + `DockerSandboxClient` | ✅ live start/stop/exec when daemon pings |
| Feature `sandbox-nanovm` → `OpsNanoVmClient` | ✅ hermetic `ops version` / start; `try_from_plan` + env-gated `ops run` |
| Feature `sandbox-kvm` → `FirecrackerKvmClient` | ✅ hermetic `firecracker --version` / start; `try_from_plan` + env-gated config boot |
| Feature `sandbox-unikernel` → `UnikernelGuestClient` + shared `unikernel/` | ✅ rootfs/kernel + `boot`/`exec`; hermetic preflight; env-gated spawn + serial exec |
| Feature `sandbox-rootfs-pack` → `unikernel::pack` checksums + live pipelines | ✅ hermetic always-on; live mkfs/virt-make-fs/docker/DockerToExt4 when env + tools; agent bake + canned pipeline |
| `SandboxClient` → `UnsupportedPlatform` (501) | ✅ fail-loud default (use feature clients for live) |
| `PlayCuaDispatcher` | ✅ **real** over injectable `PlayCuaPort` |
| Live unikernel / microVM guest process boot | ✅ env-gated (`UNIKERNEL_BOOT_INTEGRATION` / `FIRECRACKER_INTEGRATION` / `NANOVM_INTEGRATION`) |
| Guest serial/stdio exec | ✅ `unikernel::exec` + wired on unikernel/Firecracker/nanoVM clients; `UNIKERNEL_EXEC_INTEGRATION=1`; codes `_GUEST_NOT_RUNNING` / `_GUEST_IO_UNAVAILABLE` |
| Guest vsock NDJSON + Linux AF_VSOCK | ✅ `unikernel::vsock` + `sandbox-vsock`; prefer via `EIDOLON_VSOCK_CID`/`EIDOLON_VSOCK_PORT`; live `UNIKERNEL_VSOCK_INTEGRATION=1`; macOS fail-loud; protocol doc `docs/reference/unikernel-vsock-protocol.md` |
| In-guest vsock agent binary | ✅ `eidolon-vsock-agent` (`sandbox-vsock-agent`) + `unikernel::vsock_agent`; pack bake via `pack::bake_agent` / `bake_agent_then_pack` (`EIDOLON_VSOCK_AGENT`) |
| Canned rootfs pipeline (tree + bake + durable out + CLI) | ✅ `pack::canned` / `eidolon-canned-rootfs`; `EIDOLON_CANNED_ROOTFS_OUT`; hermetic stub fixture; optional agent pin URL+SHA256 |
| Published GH release Ext4 disk asset with agent | ✅ staging + pin/fetch + **default pin filled** (`rootfs-v0.1.0`; `pack::release_asset` / `eidolon-release-rootfs` / `eidolon-fetch-rootfs`) |

### PlayCua (desktop sandbox / virtual display)

**Status**: Virtual display patterns; Xvfb/Wayland integration. Dispatcher is **real** in-tree; display backends remain extraction targets.

| Module | Status | Target in Eidolon | Notes |
|--------|--------|-------------------|-------|
| `PlayCuaDispatcher` | ✅ Working | `eidolon-sandbox::playcua_dispatcher` | Injectable `PlayCuaPort`; tests use `NullPlayCuaPort` |
| Virtual display manager | ✅ **landed 2026-07-22** | `eidolon-sandbox::virtual_display` | Always-on probes + hermetic `plan_xvfb`; live Linux Xvfb behind `sandbox-virtual-display` + `XVFB_INTEGRATION=1`; VNC / Wayland compositor isolation → fail-loud codes; macOS → `_VIRTUAL_DISPLAY_UNSUPPORTED`; guide `docs/guides/virtual-display.md` |
| Display resolution/DPI config | ✅ Working | `eidolon-core::Viewport` | Reusable abstraction |

### bare-cua (lightweight containerization)

**Status**: Policy types in core; **live** Linux Landlock + cgroup v2 +
namespace unshare + seccomp-bpf hooks in `eidolon-sandbox::enforcement`
(feature-gated).

| Module | Status | Target in Eidolon | Notes |
|--------|--------|-------------------|-------|
| Landlock FS (+ best-effort TCP) | ✅ Live (Linux) | `eidolon-sandbox::enforcement` + `sandbox-landlock` | wraps `landlock` 0.4; macOS/feature-off → `EIDOLON_SANDBOX_LANDLOCK_UNSUPPORTED` |
| cgroup v2 memory/CPU | ✅ Live (Linux) | `eidolon-sandbox::enforcement` + `sandbox-cgroup` | writes `memory.max` / `cpu.max`; `disk_mib` → `io.max` rbps/wbps (device via `EIDOLON_CGROUP_DISK_DEV` / detect) or `EIDOLON_SANDBOX_CGROUP_DISK_UNAVAILABLE`; off-platform → `EIDOLON_SANDBOX_CGROUP_UNSUPPORTED` |
| Namespace unshare | ✅ Live (Linux) | `eidolon-sandbox::enforcement` + `sandbox-namespaces` | wraps `nix` 0.31 `unshare` + **PID fork** (`unshare(NEWPID)` then `fork`; child = PID 1; `pid_ns_isolates_caller` + `isolated_child_host_pid`); **USER ns** identity `/proc` maps + multi-range `/etc/subuid` via `newuidmap`/`newgidmap` (`EIDOLON_SANDBOX_NS_USER=1`; `user_ns_mapped`); **mount `pivot_root`** when `EIDOLON_SANDBOX_NS_PIVOT=1` + `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS` (`pivot_root_applied`); `EIDOLON_SANDBOX_NS_PID=1` enables plan.pid; off-platform → `EIDOLON_SANDBOX_NAMESPACES_UNSUPPORTED` |
| seccomp-bpf | ✅ Live (Linux LE) | `eidolon-sandbox::enforcement` + `sandbox-seccomp` | wraps `seccompiler` 0.5; profiles via `EIDOLON_SECCOMP_PROFILE=block-dangerous` (default) \| `oci-default` \| OCI JSON path; → `EIDOLON_SANDBOX_SECCOMP_UNSUPPORTED` |
| Resource limit **policy** | ✅ Working | `SandboxPolicy` + `plan_from_policy` | Pure translation runs on all platforms |

## Extraction Order (Phased)

### Phase 1: Foundation (High confidence, no breaking changes)
1. Extract kmobile XCTest/UiAutomator → `eidolon-mobile`
   — **T1 CLI drivers + actions + UIA2 hooks + XCUI helper + in-tree XCUITest landed**
     (`mobile-ios` / `mobile-android` / `mobile-uia2` / `mobile-xcui-helper`:
     `XcrunIosDriver` / `AdbAndroidDriver` + probes + `InstrumentDiscovery` +
     Android gated `input` + iOS `XcuiBridge` + `native/ios/EidolonXcuiHelper`
     XCUITest project + Rust `eidolon-xcui-helper` fallback +
     `Uia2Server` APK lifecycle (env/assets/cache) + `Uia2HttpClient` /
     `AppiumSessionClient` Appium-compatible session verbs + optional
     `mobile-appium` probe + `mobile-appium-desktop` Inspector/dashboard);
     full Electron Appium Desktop fork out of scope; satellites stay archived
2. Extract KVirtualStage Docker adapter → `eidolon-sandbox`
   — **T2 live bollard** behind `sandbox-docker` (`BollardDockerOrchestrator` /
     `DockerSandboxClient`); fail-loud stub retained when daemon absent;
     **T2 nanoVM/KVM CLI** behind `sandbox-nanovm` / `sandbox-kvm`; **T2
     rootfs scaffold** behind `sandbox-unikernel` (live guest boot still
     TODO); KDesktopVirt stays archived
3. Extract PlayCua Viewport/DPI logic → `eidolon-core`

### Phase 2: Desktop Platform (Medium confidence, requires refactoring)

**Operator directive 2026-06-30:** KDesktopVirt is deprecated as a standalone repo.
Full module mapping, execution DAG, and acceptance criteria live in
`docs/consolidation/KDesktopVirt-to-Eidolon.md`. The extraction order in that doc
supersedes the numbered list below.

**A+ T1 slice (2026-07-20) — landed in-tree (scaffolding + Phase E/F):**

| Item | Status |
|------|--------|
| `PhenoError::UnsupportedPlatform` + HTTP-like **501** | ✅ in `phenotype-error-core` |
| Documented codes (`EIDOLON_DESKTOP_WIN_STUB`, `_LINUX_STUB`, `_LINUX_WAYLAND_*`, `_ACTIONS_GATED`, `_WIN_CAPTURE_UNAVAILABLE`, `_RECORDING_UNAVAILABLE`, `_SECURITY_UNAVAILABLE`) | ✅ `eidolon-desktop::codes` |
| `windows` / `linux` trait hooks + fail-loud stubs | ✅ modules present |
| **Windows `SendInput` + DXGI/GDI BMP** (`desktop-windows` / `desktop-windows-dxgi`) | ✅ **landed 2026-07-21** — `WindowsClient`; DXGI Desktop Duplication preferred, GDI BitBlt fallback; gated by `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`; `EIDOLON_DESKTOP_WIN_CAPTURE=auto\|dxgi\|gdi`; miss → `EIDOLON_DESKTOP_WIN_CAPTURE_UNAVAILABLE` |
| **Windows DXGI/GDI live smoke harness** (`desktop-windows`) | ✅ **landed 2026-07-22** — env `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1` + `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`; `scaffolding` + `capture` unit tests; hermetic fail-loud off-Windows (subprocess gate); live BMP on Windows only — see `docs/guides/windows-desktop-capture.md` |
| **Linux X11 XTEST + GetImage BMP** (`desktop-linux`) | ✅ **landed 2026-07-21** — `LinuxClient`; gated by `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`; XWayland via `DISPLAY` |
| **Pure Wayland portal Screenshot** (`desktop-linux` / ashpd) | ✅ **landed 2026-07-21** — xdg-desktop-portal Screenshot + viewport (PNG IHDR); portal miss → `EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE` |
| **Wayland RemoteDesktop+Screencast absolute input** (`desktop-linux` / ashpd) | ✅ **landed 2026-07-22** — `notify_pointer_motion_absolute` / `notify_pointer_button` / `notify_keyboard_keysym`; portal miss/deny → `EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`; started session without Pointer/Keyboard/Screencast stream → `EIDOLON_DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`; **opt-in restore-token** (`EIDOLON_DESKTOP_WAYLAND_RESTORE=1`, XDG state file) skips re-prompt when portal accepts stored token; store I/O → `EIDOLON_DESKTOP_LINUX_WAYLAND_RESTORE_IO`; default (restore off) = per-call session; paste/clear = keysym chords only (no clipboard write; same as X11) |
| **Linux AT-SPI find/list/activate** (`desktop-linux-atspi`) | ✅ **landed 2026-07-21** — `LinuxAtspiAutomator` / `AtspiClient` wraps `atspi` 0.24; fail-loud `_ATSPI_STUB` / `_ATSPI_UNAVAILABLE`; activate gated; **not** full Appium Desktop GUI / W3C desktop wire |
| Full `ui_automation` port from KDesktopVirt | ❌ remaining (do not unarchive routinely) |
| `recording` + `security_hooks` trait scaffolding | ✅ fail-loud; Phase E/F complete (see sections below) |
| FFmpeg extract (Phase E) | ✅ **complete** — see Phase E below |
| Security overlap (Phase F) | ✅ **complete** — see Phase F below (no KDesktopVirt unarchive) |

4. Extract KDesktopVirt `ui_automation.rs` + `automation_engine.rs` → `eidolon-desktop`
   (Phase A — no new deps; port with unit tests)
5. Extract KDesktopVirt `metrics.rs` → `eidolon-sandbox` behind `sandbox-metrics` feature
   (Phase B — adds `sysinfo` dep)
6. Extract KDesktopVirt `containerization.rs` → `eidolon-sandbox` behind `sandbox-docker`
   feature; replaces `docker/` fail-loud stub when daemon available (Phase C — `bollard`)
   — **T2 live** `BollardDockerOrchestrator` + `DockerSandboxClient`; stub path kept for CI
7. Extract KDesktopVirt `session_storage.rs` + `audit_compliance.rs` → `eidolon-sandbox`
   (Phase D)
   — ✅ **A+ Phase D (2026-07-20)** — see Phase D section below
8. Extract KDesktopVirt `recording_pipeline.rs` + `ffmpeg_pipeline.rs` → `eidolon-desktop`
   behind `desktop-recording` feature (Phase E — high risk; ffmpeg system dep)
   — ✅ **A+ Phase E (2026-07-20)** — see Phase E section below
9. Resolve `security_framework.rs` overlap with `eidolon-core::security`; port
   non-overlapping types only (Phase F)
   — ✅ **A+ Phase F (2026-07-20)** — see Phase F section below
10. Create `crates/eidolon-ffi` with uniffi/PyO3 bindings — do NOT copy KDesktopVirt's
    unsafe C FFI wrappers (Phase G — requires separate FFI ADR)

### Phase C — Docker / bollard (A+ T2 live sandbox slice)

**Policy:** Keep KDesktopVirt **archived**. Do not unarchive. Fresh bollard wiring behind
`sandbox-docker` (no binary blobs).

| Item | Status |
|------|--------|
| Feature `sandbox-docker` | ✅ optional `bollard` + `futures-util` |
| `docker::probe` (CLI / socket, no bollard) | ✅ always-on |
| `StubDockerOrchestrator` fail-loud | ✅ default / daemon-absent |
| `BollardDockerOrchestrator` start/stop/exec/stats | ✅ when daemon pings |
| `DockerSandboxClient` (`SandboxAutomator`) | ✅ live lifecycle |
| Live integration | `DOCKER_INTEGRATION=1` + `--features sandbox-docker` |

**Remaining after T2 Docker:** none for stats surface (CPU % from deltas landed).
nanoVM/KVM CLI probes landed (see Phase 3 below); guest launch still remaining.

### Phase D — Session + audit (A+ complete, 2026-07-20)

**Policy:** Keep KDesktopVirt **archived**. Hexagonal ports from consolidation
doc + GitHub contents API read-only — do not unarchive. Strip SIEM/GDPR/HIPAA
strings that were never enforced.

| Archived KDesktopVirt concern | Eidolon destination | Phase D status |
|-------------------------------|---------------------|----------------|
| `session_storage.rs` records / TTL / name↔id index | `eidolon-sandbox::session` | ✅ `SessionRecord`, `SessionStore`, `MemorySessionStore`, fail-loud `UnavailableSessionStore` |
| File persistence (explicit path) | `FileSessionStore` | ✅ behind feature `sandbox-session` |
| Fake connection-pool / batch theatre | — | ❌ **not ported** (no SIEM / fake pool) |
| Redis adapter | `RedisSessionStore` | ✅ behind feature `sandbox-session-redis` (wraps `redis` 1.4; explicit URL + `PING`; fail-loud `EIDOLON_SANDBOX_SESSION_REDIS` / `_SESSION_IO`) |
| `audit_compliance.rs` append log | `eidolon-sandbox::audit` | ✅ `AuditEntry`, `AuditEngine`, `MemoryAuditStore`, fail-loud `UnavailableAuditStore` |
| Integrity chain | `AuditEngine::verify_integrity` | ✅ SHA-256 behind `sandbox-audit`; linkage fingerprint without feature |
| Retention purge | `RetentionPolicy` + `apply_retention` | ✅ age-based only |
| `ComplianceReport` | honest counts from stored entries | ✅ **no** fabricated SOC2/GDPR scores |
| `record_event` → audit | `AuditingSandbox` + `SandboxClient::with_audit` | ✅ composition wire; default client log-only until wired |
| Audit query indexes | `audit_index::AuditQueryIndexes` + `QueryFilter` | ✅ time / event type / actor / target / correlation; sidecar `*.jsonl.idx.json`; corrupt → `EIDOLON_SANDBOX_AUDIT_INDEX` |
| SIEM/CEF/blockchain/legal-hold engine | — | ❌ **not ported** |

**Codes:** `EIDOLON_SANDBOX_SESSION_BACKEND`, `EIDOLON_SANDBOX_SESSION_IO`,
`EIDOLON_SANDBOX_SESSION_REDIS`, `EIDOLON_SANDBOX_AUDIT_BACKEND`,
`EIDOLON_SANDBOX_AUDIT_IO`, `EIDOLON_SANDBOX_AUDIT_INDEX`.

**Tests:**
- Default `cargo test -p eidolon-sandbox --locked`: memory + unavailable unit tests
  (includes indexed query + corrupt-index fail-loud).
- File backends: `cargo test -p eidolon-sandbox --locked --features sandbox-session,sandbox-audit`
  (JSONL + `*.jsonl.idx.json` sidecar round-trip + corrupt sidecar fail-loud).
- Redis adapter: `cargo test -p eidolon-sandbox --locked --features sandbox-session-redis`
  (fail-loud unit tests always; live CRUD when `REDIS_SESSION_INTEGRATION=1` +
  `EIDOLON_REDIS_URL` / `REDIS_URL`).

**Remaining after Phase D:** none for audit query indexes (2026-07-22). Redis session
adapter ported (`sandbox-session-redis`). `SandboxClient::record_event` /
[`AuditingSandbox`] composition wire to `AuditEngine` landed (memory default
for tests; unavailable store fail-loud).

### Phase E — Recording / FFmpeg (A+ complete, 2026-07-20)

**Policy:** Keep KDesktopVirt **archived**. Do not unarchive or copy binary blobs.
Copy salvageable *patterns* only; never port `ffmpeg_pipeline_broken.rs`.

| Archived KDesktopVirt path | Eidolon destination | Phase E status |
|----------------------------|---------------------|----------------|
| `src/recording_pipeline.rs` | `crates/eidolon-desktop/src/recording/` | ✅ `RecordingPipeline` start/stop + `QualityProfile` / `VideoFormat` |
| `src/ffmpeg_pipeline.rs` | same + feature `desktop-recording` | ✅ `FfmpegRecorder` + encode / GIF argv runners |
| `src/ffmpeg_pipeline_broken.rs` | **Do NOT extract** | ❌ ignored |
| (none — system binary) | `recording::probe` (`EIDOLON_FFMPEG` / `PATH`) | ✅ no bundled ffmpeg; **fail-loud** if missing |
| RTMP / WebRTC / audio DSP | — | ❌ **not ported** (out of library scope) |
| bollard container recording | — | ❌ **not ported** (sandbox concern if ever needed) |

**Feature flag:** `eidolon-desktop` → `desktop-recording` (off by default).

**Tests:**
- Default `cargo test --locked`: stub fail-loud + argv/probe unit tests (no live capture).
- Optional: `cargo test -p eidolon-desktop --features desktop-recording` (includes
  lavfi test-pattern encode when `ffmpeg` is on PATH).
- Live capture: `RECORDING_INTEGRATION=1` (needs OS screen-recording permission).

**Remaining after Phase E:** none for the library recording surface. Container-side
recording (if desired) belongs in `eidolon-sandbox`, not a KDesktopVirt unarchive.

### Phase F — Security overlap resolution (A+ complete, 2026-07-20)

**Policy:** Keep KDesktopVirt **archived**. Audit via GitHub contents API only; do not
unarchive. Prefer `eidolon-core::security` for anything already covered.

| Archived KDesktopVirt concern | Eidolon destination | Phase F status |
|-------------------------------|---------------------|----------------|
| `ResourceLimits` / isolation resource caps | `eidolon-core::security::SandboxPolicy` + `NetworkPolicy` | ✅ **deduped** — no desktop re-definition |
| Sandbox id / exec string hygiene | `validate_sandbox_id` / `validate_exec_cmd` | ✅ **wired** via `DesktopSecurityGate` defaults |
| `AccessController` / `allowed_operations` | `eidolon-desktop::PolicySecurityGate` | ✅ **ported** allow-list + `Forbidden` |
| Failed-attempt / rate lockout | `PolicySecurityGate` sliding window | ✅ **ported** lightweight (no vault) |
| Capability token shape | `validate_capability_name` | ✅ **new** desktop-only validator |
| `EncryptedVault` / AES-GCM / Argon2 | ✅ `desktop-security-vault` | optional feature |
| `OAuthManager` / PKCE | ✅ `desktop-security-oauth` | PKCE helper (no embedded browser / JWT server) |
| `AuditLogger` / compliance export | Phase D → `eidolon-sandbox::audit` | ✅ **complete** — see Phase D |
| `SecurityEngine` monolith | — | ❌ **not ported** — compose traits |

**Tests:** `cargo test -p eidolon-desktop --locked` covers stub 501, policy allow/deny,
rate-limit, core validator delegation, and vault-capability fail-loud.

**Remaining after Phase F:** optional vault/OAuth features are landed
(`desktop-security-vault` / `desktop-security-oauth`); do **not** expand into
an embedded browser / JWT auth server in the automation library. Phase D audit
module complete (above). Live Landlock/cgroup/namespace/seccomp hooks landed
(below).

### Phase F+ — Landlock / cgroup / namespaces / seccomp (A+ live hooks)

**Policy:** Prefer wrap-over-handroll (`landlock`, `nix`, `seccompiler`). Fail-loud off Linux.

| Item | Status |
|------|--------|
| Feature `sandbox-landlock` | ✅ optional Linux `landlock` 0.4 dep |
| Feature `sandbox-cgroup` | ✅ no extra dep; cgroup v2 writes |
| Feature `sandbox-namespaces` | ✅ optional Linux `nix` 0.31 (`sched` + `signal`) |
| Feature `sandbox-seccomp` | ✅ optional Linux LE `seccompiler` 0.5 + `libc` |
| `plan_from_policy` | ✅ always-on pure translation (macOS-tested); includes ns + seccomp intent; `EIDOLON_SANDBOX_NS_PID=1` → `namespaces.pid`; `EIDOLON_SANDBOX_NS_PIVOT=1` + `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS` → `mount` + `pivot_rootfs` (fail-loud if path missing) |
| `apply_landlock` / `apply_cgroup` / `apply_namespaces` / `apply_seccomp` / `apply_isolation` | ✅ Linux+feature; else 501 codes |
| `EnforcingSandbox` / `apply_enabled_enforcement` | ✅ composition wire on `start`; PID-ns child tracked on `stop`; no features → `EIDOLON_SANDBOX_ENFORCEMENT_DISABLED` |
| Codes | ✅ `…_LANDLOCK_UNSUPPORTED`, `…_CGROUP_UNSUPPORTED`, `…_CGROUP_DISK_UNAVAILABLE`, `…_NAMESPACES_UNSUPPORTED`, `…_SECCOMP_UNSUPPORTED`, `…_ENFORCEMENT_DISABLED` |
| Disk MiB from policy | ✅ `io.max` rbps/wbps stand-in (not FS capacity); fail-loud if device/`io.max` unavailable |
| PID ns isolates caller | ✅ `unshare(NEWPID)` + `fork`; child PID 1; `NamespaceStatus.pid_ns_isolates_caller` (disk_enforced-style); parent holds `isolated_child_host_pid` |
| Seccomp OCI allowlist | ✅ `oci-default` + custom OCI/Docker JSON via `EIDOLON_SECCOMP_PROFILE`; default remains `block-dangerous` |
| USER ns uid/gid maps | ✅ identity `0 <euid> 1` via `/proc`; multi-range + `/etc/subuid` via `newuidmap`/`newgidmap` (`EIDOLON_SANDBOX_NS_USER=1`, optional `EIDOLON_SANDBOX_NS_UID_MAP` / `GID_MAP` or `EIDOLON_SANDBOX_NS_USER_SUBIDS=1`; `user_ns_mapped`) |
| Mount `pivot_root` | ✅ after `CLONE_NEWNS`: `MS_PRIVATE` + bind + `pivot_root` into `pivot_rootfs`; status `pivot_root_applied`; env `EIDOLON_SANDBOX_NS_PIVOT` + `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS`; with PID ns only the child pivots; needs `CAP_SYS_ADMIN` / user-ns; fail-loud if requested but missing path/capability |
| Integration | `LANDLOCK_INTEGRATION=1` / `CGROUP_INTEGRATION=1` / `NAMESPACES_INTEGRATION=1` / `SECCOMP_INTEGRATION=1` |

**Composition:** default `SandboxClient::start` stays fail-loud (501 stub). Wrap a
hermetic/live inner with `EnforcingSandbox` to apply policy on start. Do **not**
wrap `DockerSandboxClient` expecting guest Landlock — Docker already maps
CPU/memory via bollard host config; enforcement hooks target the **current
process**.


### Phase 3: Advanced Features (Lower priority, future)
7. Integrate KVirtualStage nanoVMs patterns
   — **T2 CLI** `sandbox-nanovm` + `OpsNanoVmClient` + `nanovm::probe` landed;
     hermetic `ops version`/start; `try_from_plan` + env-gated `ops run`;
     serial guest exec via `unikernel::exec` (`UNIKERNEL_EXEC_INTEGRATION=1`)
8. Integrate bare-cua namespace/cgroup abstractions / KVM-Firecracker
   — **T2 CLI** `sandbox-kvm` + `FirecrackerKvmClient` + `kvm::probe` landed;
     hermetic `firecracker --version`/start; `try_from_plan` + env-gated
     `--no-api --config-file` boot; serial guest exec via `unikernel::exec`
   — **Landlock + cgroup v2 + namespaces + seccomp** hooks landed under
     `enforcement` (features above)
   — **T2 rootfs scaffold** `sandbox-unikernel` + `unikernel/` (config, probe,
     `LaunchPlan`, `boot` argv/JSON, `exec` serial I/O, `vsock` NDJSON +
     Linux AF_VSOCK behind `sandbox-vsock`, `UnikernelGuestClient`)
     landed; **live process spawn** + **serial/vsock exec** env-gated;
     in-guest agent binary / production images remain (do not unarchive
     KDesktopVirt)
9. Add event serialization/playback (from agileplus-event-sourcing?)

### Phase 3b — Unikernel / rootfs scaffold (A+ T2, 2026-07-21)

**Policy:** Keep KDesktopVirt **archived**. Shared types under `unikernel/`
compose with `nanovm` / `kvm` probes — no duplicated `VirtualStage`.

| Item | Status |
|------|--------|
| Feature `sandbox-unikernel` | ✅ enables `UnikernelGuestClient` |
| `RootfsConfig` / `KernelConfig` / `UnikernelLaunchConfig` | ✅ always-on validation |
| `unikernel::probe` (`EIDOLON_ROOTFS` / `EIDOLON_KERNEL` + explicit path) | ✅ hermetic; no default home path |
| Codes `EIDOLON_SANDBOX_ROOTFS_MISSING`, `_KERNEL_MISSING`, `_UNIKERNEL_STUB` | ✅ `eidolon-sandbox::codes` |
| `LaunchPlan` composes nanovm/kvm CLI readiness | ✅ fail-loud if CLI absent |
| `unikernel::boot` plan→Firecracker/`ops` argv + config JSON | ✅ always-on; macOS-safe unit tests |
| `unikernel::exec` serial/stdio types + fail-loud codes | ✅ always-on; composes with vsock |
| `unikernel::vsock` NDJSON protocol + AF_VSOCK | ✅ framing always-on; live Linux + `sandbox-vsock` + `UNIKERNEL_VSOCK_INTEGRATION=1`; macOS fail-loud |
| Hermetic `UnikernelGuestClient::start` | ✅ re-validates artifacts + CLI |
| Env-gated live guest process spawn | ✅ `UNIKERNEL_BOOT_INTEGRATION=1` / `FIRECRACKER_INTEGRATION=1` / `NANOVM_INTEGRATION=1` |
| Env-gated serial guest exec | ✅ `UNIKERNEL_EXEC_INTEGRATION=1` + live child; codes `_GUEST_NOT_RUNNING` / `_GUEST_IO_UNAVAILABLE` |
| Env-gated vsock guest exec | ✅ `UNIKERNEL_VSOCK_INTEGRATION=1` + `EIDOLON_VSOCK_CID`/`PORT` preferred; `docs/reference/unikernel-vsock-protocol.md` |
| `FirecrackerKvmClient::try_from_plan` / `OpsNanoVmClient::try_from_plan` | ✅ compose LaunchPlan; `exec` wired (preferred transport) |
| In-guest vsock agent + production images | ✅ pack bake + canned + GH Ext4 pin (`rootfs-v0.1.0`) |

**Tests:** `cargo test -p eidolon-sandbox --locked` (+ `--features sandbox-unikernel,sandbox-kvm,sandbox-nanovm,sandbox-vsock`).

### Phase 3c — Rootfs / image packaging (A+ T2, 2026-07-21)

**Policy:** Hermetic pack of **existing** artifacts always-on. Live `mkfs.ext4` /
`virt-make-fs` / `docker export` / compose `DockerToExt4` pipelines run only with
feature `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1` + discoverable tools;
missing tools / command failures are fail-loud (never silent Ok). Do not
unarchive KDesktopVirt. See `docs/reference/rootfs-pack.md`.

| Item | Status |
|------|--------|
| Feature `sandbox-rootfs-pack` | ✅ enables SHA-256 + live pack pipelines |
| `PackageManifest` (kernel + rootfs + format + checksum) | ✅ always-on JSON schema v1 |
| `HermeticPackBuilder` (validate, stage copy/link, write manifest) | ✅ always-on; no real images required |
| Tool probes (`mkfs.ext4`, `virt-make-fs`, `docker`) | ✅ `unikernel::pack::tools`; env overrides fail-loud if set-but-missing |
| Codes `EIDOLON_SANDBOX_ROOTFS_PACK_STUB`, `_TOOL_MISSING`, `_PACK_IO` | ✅ stub = feature/env off only |
| Compose → `RootfsConfig` / `LaunchPlan` | ✅ `PackageManifest::to_rootfs_config` (+ kernel) |
| Live `mkfs.ext4` (dir → sparse image, or format existing file) | ✅ gated `ROOTFS_PACK_INTEGRATION=1` |
| Live `virt-make-fs` (directory tree → raw image) | ✅ gated `ROOTFS_PACK_INTEGRATION=1` |
| Live `docker export` (image/container → tarball + tree extract) | ✅ gated; manifest **Raw only** (never Ext4) |
| Compose `DockerToExt4` (export → tree → mkfs/virt-make-fs → Ext4 disk) | ✅ dual-tool; fail-loud if mkfs missing |
| `pack_rootfs_tree_to_ext4` (existing tree → Ext4) | ✅ same gates; no docker |
| Bake `eidolon-vsock-agent` into rootfs tree | ✅ `pack::bake_agent` / `bake_agent_then_pack`; `EIDOLON_VSOCK_AGENT`; code `_VSOCK_AGENT_MISSING` |
| Canned rootfs pipeline (durable out + CLI + stub fixture) | ✅ `pack::canned` / `eidolon-canned-rootfs`; `EIDOLON_CANNED_ROOTFS_OUT`; tree-only hermetic; Ext4 via `ROOTFS_PACK_INTEGRATION=1` |
| Optional agent pin (URL + SHA-256 cache) | ✅ scaffold (`EIDOLON_VSOCK_AGENT_URL` + `_SHA256`); no default published URL |
| GH Ext4 release staging + pin/fetch | ✅ `pack::release_asset` / `eidolon-release-rootfs` / `eidolon-fetch-rootfs`; durable `target/rootfs-release/` |
| Published GH release Ext4 disk asset (default pin filled) | ✅ `rootfs-v0.1.0` uploaded; checkout pin + SHA-256 filled |

**Tests:** `cargo test -p eidolon-sandbox --locked` (+ `--features sandbox-rootfs-pack`).
Live host tools: `ROOTFS_PACK_INTEGRATION=1` (optional `EIDOLON_PACK_DOCKER_IMAGE`).
Docs: `docs/reference/rootfs-pack.md`, `docs/guides/canned-rootfs.md`, `docs/guides/gh-ext4-release.md`.

## Dependency Graph

```
eidolon-core (no deps)
├── eidolon-desktop
├── eidolon-mobile
└── eidolon-sandbox
```

No cross-crate dependencies; each implementation consumes only `eidolon-core` traits.

## Quality Gates

- ✅ All stubs implement their trait (no unimplemented!() calls)
- ✅ Extraction target code compiles without warnings (`cargo clippy`)
- ✅ New trait methods have matching implementations in all stubs
- ✅ Event serialization round-trips (AutomationEvent → JSON → AutomationEvent)
- ✅ Each extraction has ≥1 integration test verifying trait behavior
