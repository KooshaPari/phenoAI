# ADR-002: VirtualStage Unification of DesktopAutomator / MobileAutomator / SandboxAutomator

**Date**: 2026-06-10
**Status**: Accepted
**Supersedes**: (partially) — see "Relationship to ADR-001" below.
**Superseded by**: — (none)

## Context

ADR-001 ([`Eidolon/docs/ADR-001-trait-based-core.md:36-37`](../ADR-001-trait-based-core.md)) established that Eidolon exposes **three sibling traits** as its public automation surface:

- `DesktopAutomator` ([`crates/eidolon-core/src/traits.rs:6-21`]) — 5 methods: `get_viewport`, `screenshot`, `pointer`, `text`, `record_event`. Implemented by macOS, Windows, Linux.
- `MobileAutomator` ([`crates/eidolon-core/src/traits.rs:26-44`]) — 6 methods: `get_viewport`, `screenshot`, `tap`, `swipe`, `input_text`, `record_event`. Implemented by iOS, Android.
- `SandboxAutomator` ([`crates/eidolon-core/src/traits.rs:49-67`]) — 6 methods: `get_metadata`, `start`, `stop`, `exec`, `resource_usage`, `record_event`. Implemented by Docker, nanoVMs, KVM, Firecracker.

The three traits are **siblings, not super/sub** ([`crates/eidolon-core/src/traits.rs:6-67`]). They share three methods (`get_viewport`, `screenshot`, `record_event`) and diverge on the rest. This produces three problems as the platform-impl surface grows:

1. **Redundant dispatch surface for consumers.** A consumer that wants to "automate a device" must bind to the *correct* trait and may need three different handles (`Arc<dyn DesktopAutomator>`, `Arc<dyn MobileAutomator>`, `Arc<dyn SandboxAutomator>`) for a multi-modal device such as a ChromeOS VM with both a desktop window manager and a sandbox container. There is no common type for "an automatable surface".
2. **Divergent method names for shared semantics.** `DesktopAutomator::pointer` and `MobileAutomator::tap` are *the same operation* (dispatch an input event at coordinates) on different platforms, but the trait surface exposes them under different signatures, forcing consumers to write platform-specific dispatch tables.
3. **Weak conformance story.** A consumer cannot write `for stage in &stages { stage.screenshot(...).await }` because the three traits are disjoint — each `&dyn` reference is monomorphized to a single trait. Conformance testing against all platform impls therefore requires per-trait test boilerplate.

The binding directive in `PHENOTYPE_5REPO_MODERNIZATION_PLAN.md` §8.1 Q1.1 resolves this directly:

> *"Keep 3 platform impls real (macOS, Windows, Linux as core surfaces; Android, iOS as sub-features of mobile); abstract everything else behind a single `VirtualStage` trait."*

The trait is the load-bearing artifact. The thin platform impls (today: 1 real, 8+ stubs across `eidolon-desktop`/`-mobile`/`-sandbox`; see `plans/2026-06-09-eidolon-platform-impl-plan-v1.md` §3) are a separate, lower-priority workstream and are out of scope for this decision.

## Decision

Introduce **`VirtualStage`** as the unified surface for device automation. `VirtualStage` is a single `#[async_trait::async_trait]` trait declared in [`crates/eidolon-core/src/traits.rs`] with:

- **5 required methods** (the universal subset — every automatable surface has these):
  - `get_viewport(&self) -> Result<Viewport>`
  - `screenshot(&self, path: &str) -> Result<()>`
  - `pointer(&self, event: &PointerInput) -> Result<()>`
  - `text(&self, event: &TextInput) -> Result<()>`
  - `record_event(&self, event: AutomationEvent) -> Result<()>`
- **8 optional methods** (the divergent subset — every method is provided with a sensible default that allows the trait to be uniformly implemented on a desktop, mobile, or sandbox impl):
  - `tap(&self, x: i32, y: i32) -> Result<()>` — default: forwards to `pointer` (press+release).
  - `swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()>` — default: move + click sequence.
  - `input_text(&self, text: &str) -> Result<()>` — default: forwards to `text(keystroke)`.
  - `exec(&self, _cmd: &str) -> Result<String>` — default: `Err(PhenoError::Unsupported("exec"))` on desktop/mobile; overridden on sandbox.
  - `resource_usage(&self) -> Result<ResourceUsage>` — default: zeros on desktop/mobile; overridden on sandbox.
  - `start(&self) -> Result<()>` and `stop(&self) -> Result<()>` — default: `Ok(())` on desktop/mobile; overridden on sandbox.
  - `get_metadata(&self) -> Result<SandboxMetadata>` — default: `SandboxMetadata { id: "virtual-stage", image: "n/a", ... }` on desktop/mobile; overridden on sandbox.

Introduce **two sub-traits** that capture the platform-specific shapes as type-narrowing wrappers:

- `MobileStage: VirtualStage` — adds `run_test` and `dumpsys_viewport`. Subsumes the existing `IosTestAdapter` and `AndroidTestAdapter` placeholders ([`crates/eidolon-mobile/src/native/mod.rs:8-23`]).
- `SandboxStage: VirtualStage` — adds `start_container`, `stop_container`, `get_container_resource_usage`. Subsumes the existing `DockerOrchestrator` placeholder ([`crates/eidolon-sandbox/src/docker/mod.rs:9-18`]).

**Backward compatibility.** The three historical traits (`DesktopAutomator`, `MobileAutomator`, `SandboxAutomator`) **remain in `eidolon-core`** — no removal in this decision. They become blanket-impl super-traits of `VirtualStage`:

```rust
impl<T: VirtualStage + ?Sized> DesktopAutomator for T { /* delegate to VirtualStage */ }
```

Existing consumers that typed their handle as `Arc<dyn DesktopAutomator>` (or `MobileAutomator` / `SandboxAutomator`) continue to compile. The 31 + 15 + 18 + 19 = 83 existing tests across `eidolon-core`, `eidolon-desktop`, `eidolon-mobile`, `eidolon-sandbox` pass unchanged.

**Cross-platform fallback dropped.** The unstated `pub struct DesktopClient { platform: String }` stub at [`crates/eidolon-desktop/src/lib.rs:18-58`] (the `cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))` fallback for BSDs/illumos/wasm, none of which the workspace targets) is removed. If a future need arises, it gets its own `eidolon-wasm-stage` subcrate.

## Rationale

**Q1.1 binding directive.** `PHENOTYPE_5REPO_MODERNIZATION_PLAN.md` §8.1 Q1.1 names `VirtualStage` as the load-bearing artifact for Eidolon. Without it, the 5 sibling projects (KDesktopVirt, kmobile, KVirtualStage, PlayCua, bare-cua) cannot converge on a single consumer-side handle.

**Single consumer handle.** A consumer writes:

```rust
let stage: Arc<dyn VirtualStage> = eidolon_desktop::MacOSClient::new("macos")?;
stage.screenshot("/tmp/before.png").await?;
stage.pointer(&PointerInput::click(100, 200)).await?;
stage.screenshot("/tmp/after.png").await?;
```

The same code runs against `MacOSClient`, `WindowsClient`, `LinuxClient`, `MobileClient`, and `SandboxClient`. On a sandbox impl the consumer can additionally call `stage.exec("ls")` because `VirtualStage` exposes it; on a desktop impl that call returns `Unsupported` — the type system tells the consumer that the operation is sandbox-only at *runtime*, uniformly through the same handle.

**Layered sub-traits.** `MobileStage: VirtualStage` and `SandboxStage: VirtualStage` provide concrete defaults for the methods that need platform-specific behaviour (XCTest tap, Docker exec, etc.). Leaf impls override only what they need; the rest of `VirtualStage` is satisfied by the sub-trait's blanket defaults.

**FFI surface.** A `VirtualStage` vtable is the natural binding target for PyO3 / uniffi / napi-rs per `PHENOTYPE_5REPO_MODERNIZATION_PLAN.md` §8.6. Python, Node, Swift, Kotlin consumers see a single API; the leaf sub-traits (`MobileStage`, `SandboxStage`) are exposed as optional methods via binding-side wrappers.

**Conformance test surface.** A single `cargo test --workspace` conformance suite in `eidolon-core/tests/test_virtual_stage.rs` enumerates all registered `VirtualStage` impls and runs a uniform test matrix (viewport, screenshot, pointer, text, record_event). The optional methods are tested with `#[cfg(feature = "sandbox")]` / `#[cfg(feature = "mobile")]` gates. This collapses what would otherwise be 3 separate per-trait conformance suites into one.

**Backward compat preserves 83 tests.** ADR-001's only concrete deliverable so far is the three traits + 83 passing tests. Deleting the three traits in a major-version bump would force re-typing every test; keeping them as blanket-impl super-traits costs 18 lines of trait boilerplate (`impl<T: VirtualStage + ?Sized> DesktopAutomator for T { ... }` × 3) and zero test churn. Backward compat is the cheap path.

## Consequences

### Positive

- **Single trait surface.** One `Arc<dyn VirtualStage>` handle covers desktop, mobile, and sandbox. Consumers stop writing per-trait dispatch tables.
- **Layered sub-traits.** `MobileStage` and `SandboxStage` are explicit type-narrowing wrappers; platform-specific methods (`run_test`, `start_container`, etc.) are no longer hidden in `native/` placeholders.
- **Uniform conformance testing.** A single test matrix in `eidolon-core/tests/test_virtual_stage.rs` exercises every registered impl, replacing three per-trait test files.
- **FFI-ready.** The 5-required / 8-optional shape is the natural surface for uniffi / PyO3 / napi-rs bindings.
- **Backward compat.** 83 existing tests pass unchanged. The three historical traits remain available for consumers that need them.
- **Cross-platform fallback removed.** 41 lines of unstated stub code in `eidolon-desktop/src/lib.rs:18-58` go away; the workspace compiles for the three targets it actually supports.

### Negative

- **Virtual dispatch overhead.** `Arc<dyn VirtualStage>` is dynamic dispatch through a vtable. Each method call pays a vtable indirection (~1-2 ns) plus a heap allocation for the `Arc`. For the macOS screenshot path (which itself is ~10-50 ms of Core Graphics I/O) this is in the noise. For the high-frequency pointer/text path on Linux/mobile (where consumers may issue 100+ events/sec) the overhead is real but bounded.
- **Optional method default-impl opacity.** A consumer calling `stage.exec("ls")` on a desktop impl gets `Err(Unsupported)`, not a compile error. The optional-with-default shape is uniform but loses type safety compared to making `exec` *required* on a `SandboxStage` sub-trait. The trade-off is intentional: uniform `Arc<dyn VirtualStage>` handle in exchange for runtime errors on platform-incompatible operations. Consumers that need type safety can downcast (`Arc::downcast`) or bind to the sub-trait (`Arc<dyn SandboxStage>`).
- **One more trait to learn.** `VirtualStage` joins the three historical traits in the public API surface. The 5-required / 8-optional shape must be documented. Mitigation: ADR-002 (this document) + the updated `Eidolon/README.md` + the `CLAUDE.md` Trait Design section.

### Mitigations

- **Static-dispatch optimization path.** The `#[async_trait::async_trait]` wrapper boxes the future. For consumers that want zero overhead, the impl types (`MacOSClient`, `LinuxClient`, `MobileClient`, `SandboxClient`) remain concrete `pub struct`s; the dynamic dispatch is opt-in via `Arc<dyn VirtualStage>`. The future `eidolon` 0.2 plan (out of scope for this ADR) can add a `StageEnum` static-dispatch wrapper (`enum Stage { Mac(MacOSClient), Win(WindowsClient), ... }`) for the inner loop while keeping `VirtualStage` as the outer consumer-side abstraction.
- **Optional-method documentation.** Each default-impl on `VirtualStage` carries a doc comment naming the platform(s) for which it is meaningful and the platform(s) for which it returns `Err(Unsupported)`.

## Alternatives Considered

### Alternative A: Delete the three historical traits outright (breaking change)

Replace `DesktopAutomator` / `MobileAutomator` / `SandboxAutomator` with `VirtualStage`; require every consumer to retype their handle.

**Rejected.** Forces re-typing all 83 existing tests. Breaks any out-of-tree consumer that has bound to the historical traits. The blanket-impl super-trait pattern preserves the existing surface at trivial cost (18 lines of boilerplate).

### Alternative B: Make the divergent methods *required* on a `SandboxStage` sub-trait, omit them from `VirtualStage`

The 8 optional methods become 8 methods split between `MobileStage` and `SandboxStage`. `VirtualStage` exposes only the 5 universal methods.

**Rejected.** Loses the uniform `Arc<dyn VirtualStage>` handle: a consumer that wants to call `stage.exec("ls")` on a sandbox impl must first downcast or bind to `Arc<dyn SandboxStage>`. The optional-with-default approach is more uniform at the cost of one `Err(Unsupported)` per cross-platform call site.

### Alternative C: Enum dispatch (`enum Stage { Mac, Win, Linux, Mobile, Sandbox }`)

Replace the trait with a `match`-driven enum. No vtable cost.

**Rejected.** The `PHENOTYPE_5REPO_MODERNIZATION_PLAN.md` §8.1 Q1.1 directive names the trait as the load-bearing artifact. The enum approach trades dynamic dispatch for a closed set of variants — adding a new platform (e.g., a future web-automation impl) requires modifying the enum and every match arm. The trait is open for extension.

### Alternative D: Keep the three historical traits, do not introduce `VirtualStage`

Do nothing. Consumers continue to bind to one of three traits.

**Rejected.** Q1.1 explicitly requires the unified surface. ADR-001's sibling-traits shape is the *historical* decision; this ADR is the *correction* to it. The historical traits remain, but `VirtualStage` becomes the recommended public surface.

## Reference

- **Plan document:** `plans/2026-06-09-eidolon-platform-impl-plan-v1.md` §9 ("VirtualStage Trait — Proposal"), §10 Task 1-7
- **Binding directive:** `PHENOTYPE_5REPO_MODERNIZATION_PLAN.md` §8.1 Q1.1
- **Existing trait surface:** `Eidolon/crates/eidolon-core/src/traits.rs:1-85`
- **Trait file to modify (proposed):** `Eidolon/crates/eidolon-core/src/traits.rs` — add `VirtualStage` after line 85
- **MADR template:** `Eidolon/docs/adr/0001-record-architecture-decisions.md:1-20`
- **Prior ADR (sibling traits):** `Eidolon/docs/ADR-001-trait-based-core.md:36-37`
- **macOS impl (real, refactor target):** `Eidolon/crates/eidolon-desktop/src/macos.rs:49-227`
- **Windows impl (stub, refactor target):** `Eidolon/crates/eidolon-desktop/src/windows.rs:25-59`
- **Linux impl (stub, refactor target):** `Eidolon/crates/eidolon-desktop/src/linux.rs:25-60`
- **Mobile impl (stub, refactor target):** `Eidolon/crates/eidolon-mobile/src/lib.rs:27-57`
- **Sandbox impl (stub, refactor target):** `Eidolon/crates/eidolon-sandbox/src/lib.rs:26-61`

## Relationship to ADR-001

ADR-001 ([`Eidolon/docs/ADR-001-trait-based-core.md`](../ADR-001-trait-based-core.md)) established the three sibling traits. This ADR does **not** supersede ADR-001 — the rationale for *trait-based* design (over direct code merge) is unchanged. ADR-002 refines the trait *shape*: the three siblings are unified behind `VirtualStage`, with `MobileStage` and `SandboxStage` as type-narrowing sub-traits and the historical three traits preserved as backward-compat blanket-impl super-traits.

ADR-001 §"Decision" is therefore amended as follows: **add** the second sentence *"Implementations of any platform expose `VirtualStage` as the unified surface. The three sibling traits remain as backward-compat super-traits; consumers that bind to them continue to compile."*
