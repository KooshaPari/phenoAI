//! Criterion benchmark — `VirtualStage` dyn dispatch overhead.
//!
//! `VirtualStage` (see `crates/eidolon-core/src/virtual_stage.rs`) is the
//! unified async trait a consumer holds to drive any platform impl
//! (macOS / Windows / Linux desktop, iOS / Android mobile, Docker /
//! nanoVMs / KVM sandbox). The trait is `#[async_trait]`-based and
//! designed to be reached through `Box<dyn VirtualStage>` (or
//! `Arc<dyn VirtualStage>`) — i.e. through a vtable.
//!
//! The trait unification in `plans/2026-06-09-eidolon-platform-impl-plan-v1.md`
//! §9.1 was specifically motivated by heterogeneous consumer code
//! (e.g. a ChromeOS VM that is both a desktop window manager and a
//! sandbox container). The cost of that unification is a vtable
//! indirection on every trait call. This benchmark measures that cost
//! against the static-dispatch baseline and against the realistic
//! `Vec<Box<dyn VirtualStage>>` use case (5 different platform impls
//! in a single vector, the consumer iterates and calls `get_viewport`
//! on each).
//!
//! Three measurements:
//!
//! 1. **`static_dispatch_get_viewport`** — baseline. Calls
//!    `get_viewport` directly on a concrete `MacOSClient` value. This
//!    goes through the monomorphised inherent / trait impl with no
//!    vtable and no allocation.
//!
//! 2. **`dyn_dispatch_get_viewport`** — single `Box<dyn VirtualStage>`
//!    over a `MacOSClient`. This is the vtable-cost floor for the new
//!    trait surface: one indirection through the `dyn` vtable per
//!    call.
//!
//! 3. **`dyn_dispatch_heterogeneous_vec`** — `Vec<Box<dyn VirtualStage>>`
//!    of 5 different platform impls (macOS, Windows, Linux, Mobile,
//!    Sandbox), calling `get_viewport` on each in a loop. This is the
//!    realistic consumer pattern: a multi-modal consumer holds a
//!    heterogeneous vector of stages and iterates them.
//!
//! The expected outcome is that `(2) / (1)` reports the vtable
//! overhead of `VirtualStage::get_viewport` (single-call) and that
//! `(3) / (1)` reports the heterogeneous-vector overhead (the same
//! vtable cost, plus the iteration bookkeeping). The point is to put
//! a number on the abstraction so future trait changes (e.g. moving
//! off `async_trait` to native `async fn in trait`, or splitting
//! `VirtualStage` into smaller sub-traits) can be evaluated against a
//! concrete baseline.
//!
//! Run with:
//! ```bash
//! cd Eidolon && cargo bench -p eidolon-core virtual_stage
//! ```
//!
//! Mock clients are local to this bench file (same shape as
//! `tests/test_virtual_stage.rs`); the real platform impls in
//! `eidolon-desktop` / `eidolon-mobile` / `eidolon-sandbox` are
//! conditionally compiled and not reachable from `eidolon-core`
//! benches. The mocks are zero-sized unit structs, so the only cost
//! measured here is the dispatch shape — not the I/O of a real
//! platform call.

use async_trait::async_trait;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eidolon_core::{
    AutomationEvent, MobileStage, PointerInput, Result, SandboxStage, TextInput, Viewport,
    VirtualStage,
};

// -- Mock platform clients ----------------------------------------------------
//
// Five unit-struct mocks, one per platform. The trait impls are
// minimal: every method returns `Ok(_)` with a `Viewport::desktop_fhd()`
// (or `mobile_fhd()` for the mobile impl) so the inner work is
// constant and small relative to the dispatch itself. The benchmarks
// are interested in the *shape* of the call, not the work inside it.

/// Mock macOS desktop client.
struct MacOSClient;
/// Mock Windows desktop client.
struct WindowsClient;
/// Mock Linux desktop client.
struct LinuxClient;
/// Mock mobile client (iOS / Android).
struct MobileClient;
/// Mock sandbox client (Docker / nanoVMs / KVM).
struct SandboxClient;

#[async_trait]
impl VirtualStage for MacOSClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Ok(())
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Ok(())
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Ok(())
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl VirtualStage for WindowsClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Ok(())
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Ok(())
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Ok(())
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl VirtualStage for LinuxClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Ok(())
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Ok(())
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Ok(())
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl VirtualStage for MobileClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::mobile_fhd())
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Ok(())
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Ok(())
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Ok(())
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

// `MobileStage` sub-trait — empty impl block, exercises the default
// no-op impls. Not strictly needed for the get_viewport benchmark but
// kept so the mock mirrors the shape used in
// `tests/test_virtual_stage.rs`.
#[async_trait]
impl MobileStage for MobileClient {}

#[async_trait]
impl VirtualStage for SandboxClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Ok(())
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Ok(())
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Ok(())
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

// `SandboxStage` sub-trait — empty impl block, exercises the default
// no-op / zero-valued impls.
#[async_trait]
impl SandboxStage for SandboxClient {}

// -- Bench harness ------------------------------------------------------------
//
// A single-threaded tokio runtime is built once per bench and reused
// across iterations. `block_on` is the cheapest way to drive an
// `async fn` from a sync `criterion` `b.iter` closure; the cost of
// runtime construction is paid in the bench setup, not the
// measurement. The same runtime is used for all three benches so
// the comparison is apples-to-apples (the cost of the runtime is
// identical in every measurement, so it cancels in the ratio).

/// Build the tokio runtime used by every bench in this file.
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("eidolon-core benches: tokio current-thread runtime must build")
}

/// (1) Static dispatch baseline.
///
/// Calls `get_viewport` directly on a concrete `MacOSClient` value.
/// This is the monomorphised / inherent dispatch path with no vtable,
/// no `Box`, and no `dyn` indirection. The number this reports is the
/// "what would a non-trait API cost" floor.
fn static_dispatch_get_viewport(c: &mut Criterion) {
    let stage = MacOSClient;
    let rt = runtime();

    c.bench_function("static_dispatch_get_viewport", |b| {
        b.iter(|| {
            // `block_on` drives the `async fn`; the result is fed to
            // `black_box` so the optimiser does not constant-fold
            // the entire body away.
            let viewport = rt.block_on(stage.get_viewport()).unwrap();
            black_box(viewport);
        });
    });
}

/// (2) Single `Box<dyn VirtualStage>` — vtable indirection floor.
///
/// Holds one `Box<dyn VirtualStage>` over a `MacOSClient` and calls
/// `get_viewport` through the `dyn` vtable. This is the vtable-cost
/// floor for the new trait surface: exactly one indirection per
/// call. The ratio `(2) / (1)` is the per-call overhead of
/// `#[async_trait]` + `Box<dyn VirtualStage>` for `get_viewport`.
fn dyn_dispatch_get_viewport(c: &mut Criterion) {
    let stage: Box<dyn VirtualStage> = Box::new(MacOSClient);
    let rt = runtime();

    c.bench_function("dyn_dispatch_get_viewport", |b| {
        b.iter(|| {
            let viewport = rt.block_on(stage.get_viewport()).unwrap();
            black_box(viewport);
        });
    });
}

/// (3) Heterogeneous `Vec<Box<dyn VirtualStage>>` — realistic consumer pattern.
///
/// Holds 5 different platform impls in a single vector and calls
/// `get_viewport` on each in a loop. This mirrors the consumer
/// pattern documented in
/// `plans/2026-06-09-eidolon-platform-impl-plan-v1.md` §10.2
/// ("multi-modal consumer"): one consumer handle that drives
/// desktop / mobile / sandbox stages uniformly. The ratio
/// `(3) / (1)` divided by 5 reports the per-call overhead in the
/// realistic fan-out, including the vector-iteration bookkeeping
/// (`Box<dyn VirtualStage>` pointer chase, no-op fat-pointer copy).
fn dyn_dispatch_heterogeneous_vec(c: &mut Criterion) {
    let stages: Vec<Box<dyn VirtualStage>> = vec![
        Box::new(MacOSClient),
        Box::new(WindowsClient),
        Box::new(LinuxClient),
        Box::new(MobileClient),
        Box::new(SandboxClient),
    ];
    let rt = runtime();

    c.bench_function("dyn_dispatch_heterogeneous_vec", |b| {
        b.iter(|| {
            for stage in stages.iter() {
                let viewport = rt.block_on(stage.get_viewport()).unwrap();
                black_box(viewport);
            }
        });
    });
}

criterion_group!(
    benches,
    static_dispatch_get_viewport,
    dyn_dispatch_get_viewport,
    dyn_dispatch_heterogeneous_vec
);
criterion_main!(benches);
