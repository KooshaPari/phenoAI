# WBS — Eidolon (2026-08-09)

**Repo:** Eidolon
**Status:** initial skeleton; expand as scope grows.
**Owner:** forge (agent CLI). **Driver:** `proc` / `proc <id>`.

## Phase overview

Eidolon is the multi-platform agent runtime (macOS desktop, Windows
desktop, Linux desktop, mobile iOS/Android). Work is currently at
~90% per the README work-state banner — A+ mobile XCUI bridge +
Android input actions + LaunchPlan→guest boot + serial guest exec +
namespaces/seccomp + EnforcingSandbox + Phase D/E/F + T2 Docker /
nanoVM / KVM / unikernel.

| Phase | Tasks | Theme | Outcome |
|-------|-------|-------|---------|
| 0 | 1–5 | audit close-out + inventory | reproducible baseline |
| 1 | 6–15 | macOS desktop (`desktop-macos`) hardening | XCUI + Metal verified |
| 2 | 16–25 | Windows desktop (`desktop-windows`) parity | SendInput + DXGI/GDI verified |
| 3 | 26–35 | Linux desktop (`desktop-linux`) parity | X11 + Wayland portal verified |
| 4 | 36–50 | mobile iOS/Android (T1) parity | xcrun / adb gated-input verified |
| 5 | 51–60 | Phase D audit + Phase E recording | session replay works |
| 6 | 61–75 | Phase F security (Landlock/cgroup/seccomp + EnforcingSandbox) | gate always-on |
| 7 | 76–85 | T2 Docker + nanoVM/KVM + unikernel boot | 3 backends verified |

---

## Phase 0 — Audit (tasks 1–5)

| ID | Title | depends_on | ac |
|----|-------|------------|----|
| 1 | inventory desktop-* + mobile-* + T2-* runtime crates | — | ac_v1 |
| 2 | scan remaining clippy warnings | 1 | ac_v1 |
| 3 | scan bandit MEDIUM findings (Python glue) | 1 | ac_v1 |
| 4 | scan F841 unused-name findings | 1 | ac_v1 |
| 5 | tag current HEAD as `eidolon-v0.x` baseline | 1–4 | ac_v1 |

---

## Ac conventions

- `ac_v1`: commit on `main` with conventional subject + DAG id in footer.
- `ac_test`: `cargo test` exits 0.
- `ac_clippy`: `cargo clippy -- -D warnings` exits 0.

---

## Notes

- Part of the **Phenotype Fleet** (cross-repo audit at
  `pheno-harness/_cockpit/XREPO_BACKLOG.json`).
- AMC / Agentora remains paused per `pheno-harness/AGENTS.md §3.2`.
- Branch taxonomy: 8-prefix (feat/, fix/, chore/, docs/, test/, refactor/,
  perf/, build/).
