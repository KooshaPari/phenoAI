# Eidolon API Reference

## Core Traits

### `VirtualStage`
```rust
#[async_trait]
pub trait VirtualStage: Send + Sync {
    async fn get_viewport(&self) -> Result<Viewport>;
    async fn screenshot(&self, path: &str) -> Result<()>;
    async fn pointer(&self, event: &PointerInput) -> Result<()>;
    async fn text(&self, event: &TextInput) -> Result<()>;
    async fn record_event(&self, event: AutomationEvent) -> Result<()>;
    fn boxed(self) -> Box<dyn VirtualStage> where Self: Sized + 'static;
}
```
Unified automation surface for desktop, mobile, and sandbox.

```rust
let stage: Arc<dyn VirtualStage> = MacDesktopStage::new().boxed().into();
let vp = stage.get_viewport().await?;
stage.screenshot("/tmp/frame.png").await?;
```

### `DesktopAutomator`
Same method set as `VirtualStage`. Platform-specific: macOS, Windows, Linux.

### `MobileAutomator`
Extends with `tap(x, y)`, `swipe(x1, y1, x2, y2)`, `input_text(text)`. iOS/Android.

### `SandboxAutomator`
Extends with `start()`, `stop()`, `exec(cmd)`, `get_metadata()`, `resource_usage()`. Docker/nanoVMs/KVM.

## Types

### `Viewport`
```rust
pub struct Viewport { pub width: u32, pub height: u32, pub dpr: f64, pub orientation: String }
```
Display dimensions. Presets: `desktop_fhd()`, `mobile_fhd()`, `tablet_qhd()`.

### `SandboxPolicy`
```rust
pub struct SandboxPolicy {
    pub cpu_cores: u32, pub memory_mib: u32, pub disk_mib: Option<u32>, pub network: NetworkPolicy
}
```
Resource limits. Default: 2 cores, 512 MiB RAM, 5 GiB disk, no network.

### `ResourceUsage`
```rust
pub struct ResourceUsage { pub cpu_percent: f64, pub memory_mb: u32, pub disk_mb: Option<u32> }
```

### `NetworkPolicy`
```rust
pub enum NetworkPolicy { Allow, Deny, EgressAllowList }
```
