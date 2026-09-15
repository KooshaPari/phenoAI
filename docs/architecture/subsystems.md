# Subsystems — Eidolon

ADR-038 cross-link: see [ADR-038: Hexagonal port-adapter L4 policy](https://github.com/KooshaPari/phenotype-apps/blob/main/docs/adr/2026-06-18/ADR-038-hexagonal-port-adapter-l4-policy.md) for the canonical input/output port contract.

> L7 subsystem decomposition. Bounded contexts, ports, owned data, external
> dependencies, and failure modes for the Phenotype-org agent runtime.
> Companion to `ARCHITECTURE.md` and `docs/adr/`. Initial decomposition
> 2026-06-21 (v16 cycle-6 T1).

## Subsystem map

| Subsystem | Crate | Responsibility | Owned data | Critical? |
|---|---|---|---|---|
| Core runtime | `eidolon-core` | Trait surface (`Stage`, `Transport`, `Intent`), shared types, error model, intent router | `Intent`, `Stage`, `Transport`, `TransportError`, router config | yes |
| Desktop stage | `eidolon-desktop` | Concrete desktop implementation: window, IPC, native menubar, file-system access | desktop session state, native window handles, keychain tokens | yes |
| Mobile stage | `eidolon-mobile` | iOS/Android backend for Eidolon: device drivers, sandboxed file IO, push hooks | device session, mobile keychain, push tokens | yes |
| Sandbox | `eidolon-sandbox` | Process-level isolation for untrusted intent handlers (Landlock/seccomp on Linux, sandboxd on macOS) | sandbox profiles, capability tokens | no (opt-in) |
| CLI | (workspace binary) | CLI entry-point that wires `core` to a chosen stage | argv config, log file path | no |

## Port catalogue

### Input ports (consumed)

- `pheno-errors::Error` — standardized error envelope (ADR-025 family).
- `pheno-config` `Config` — layered config (12-factor cascade).
- `pheno-tracing` `Tracer` — OTLP export of stage transitions.
- `mobile-mcp::EidolonTransport` — external MCP integration (shim, see `EIDOLON_SHIM.md`).
- `mobile-cli` --eidolon-endpoint flag — CLI dispatch bridge.

### Output ports (produced)

- `eidolon-core::Stage` — public trait implemented by `eidolon-desktop` / `eidolon-mobile`.
- `eidolon-core::Transport` — trait implemented by LocalTransport, RemoteTransport, NullEidolonTransport.
- `eidolon-core::IntentRouter` — routes parsed intents to handlers.
- JSON-line worklog stream consumed by `pheno-worklog-schema` validators.

## External dependencies

| Dependency | Kind | Used by |
|---|---|---|
| `pheno-config` | Cargo path (workspace `pheno-config`) | runtime config cascade |
| `pheno-errors` | Cargo path | error envelope |
| `pheno-tracing` | Cargo path | OTLP spans |
| `mobile-mcp` | external (npm `@phenotype/mobile-mcp`) | device automation |
| `mobile-cli` | external (Go binary, invoked via CLI) | device discovery |
| Linux: `landlock` crate (`sandbox-landlock`) | OS LSM (eidolon-sandbox::enforcement) | FS (+ best-effort TCP) isolation |
| Linux: cgroup v2 (`sandbox-cgroup`) | `/sys/fs/cgroup` writes | memory.max / cpu.max |
| Linux: namespaces (`sandbox-namespaces`) | `nix::sched::unshare` + `fork` + `pivot_root` | UTS/IPC/cgroup/net; PID → child PID 1; mount pivot via `EIDOLON_SANDBOX_NS_PIVOT` |
| Linux: seccomp (`sandbox-seccomp`) | `seccompiler` BPF | `EIDOLON_SECCOMP_PROFILE`: `block-dangerous` (default) \| `oci-default` \| OCI JSON path |
| Linux: `x11rb` + `ashpd` (`desktop-linux`) | X11 XTEST/GetImage; Wayland portal Screenshot + RemoteDesktop/Screencast inject | desktop stage capture / inject |
| macOS: `sandbox-exec` | OS facility | **not shipped** — Linux hooks fail-loud |

## Failure modes

| Subsystem | Failure | Detection | Recovery |
|---|---|---|---|
| Core | Trait mismatch across versions | compile-time | Cargo workspace version pin |
| Desktop | Window server disconnect (Wayland/X11) | `TransportError::BackendGone` | reconnect with backoff |
| Mobile | Device disconnect | iOS/Android reachability check | rediscovery via mobile-mcp |
| Sandbox | Landlock/cgroup/ns/seccomp unsupported (macOS / feature-off / kernel) | `PhenoError::UnsupportedPlatform` + `EIDOLON_SANDBOX_*_UNSUPPORTED` | do not pretend isolation; surface 501 |
| Sandbox | Landlock restrict_self / cgroup write / unshare / seccomp apply fails | `PhenoError::Platform` or unsupported code | surface as `StageError`; no silent skip |
| Core | Intent unparseable | `IntentError::Parse` | fallback to LLM router; structured error returned to caller |
| Mobile | Push token revoked | 410 Gone from APNs/FCM | refresh; emit `worklog` event |
| Sandbox | capability token expired | token TTL check at spawn | request renewal from supervisor |

## Change log

- 2026-07-21 — Mount `pivot_root`: after `CLONE_NEWNS`, bind + `pivot_root`
  into `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS` when `EIDOLON_SANDBOX_NS_PIVOT=1`;
  `pivot_root_applied` (disk_enforced-style); PID-ns child only pivots.
- 2026-07-21 — Pure Wayland portal Screenshot (`ashpd` / xdg-desktop-portal)
  on `desktop-linux`; `wayland_ready()` true; portal miss →
  `EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`.
- 2026-07-22 — Pure Wayland RemoteDesktop+Screencast absolute pointer /
  keysym keyboard inject (`ashpd`); portal miss/deny →
  `EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`; device/stream gap →
  `EIDOLON_DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`.
- 2026-07-22 — Virtual display manager (`eidolon-sandbox::virtual_display`):
  always-on Xvfb/VNC/Wayland probes; hermetic `plan_xvfb`; live Linux Xvfb
  behind `sandbox-virtual-display` + `XVFB_INTEGRATION=1`; VNC / compositor
  isolation fail-loud; macOS → `EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED`.
- 2026-07-21 — PID ns fork: `unshare(NEWPID)` + `fork` so child is PID 1;
  `pid_ns_isolates_caller` / `isolated_child_host_pid`; `EIDOLON_SANDBOX_NS_PID`.
- 2026-07-21 — `AuditingSandbox` / `SandboxClient::with_audit` wire
  `record_event` → `AuditEngine` (memory default for tests; unavailable
  store fail-loud). Default client without audit stays log-only.
- 2026-07-21 — Namespace unshare + seccomp-bpf hooks (`sandbox-namespaces` /
  `sandbox-seccomp`); macOS fail-loud codes; composed into
  `apply_enabled_enforcement`.
- 2026-07-21 — `EnforcingSandbox` / `apply_enabled_enforcement` wire policy
  apply on lifecycle `start` (feature opt-in; macOS fail-loud).
- 2026-07-20 — Landlock + cgroup v2 enforcement hooks (`sandbox-landlock` /
  `sandbox-cgroup`); macOS fail-loud codes documented.
- 2026-06-21 — initial decomposition (v16 cycle-6 T1, L7).