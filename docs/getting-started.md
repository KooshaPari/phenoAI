# Eidolon Getting Started

## Why Eidolon?

Eidolon unifies device automation across desktop, mobile, and sandboxed environments with a single trait-based API. Whether you're automating UI testing on macOS, driving mobile app flows on iOS/Android, or executing commands in isolated container environments, Eidolon abstracts platform differences so your automation logic is portable and composable.

**Key problems Eidolon solves:**

- **Platform-agnostic automation** — Write one automation script; run on desktop (macOS/Windows/Linux), mobile (iOS/Android), or sandbox (Docker/nanoVMs)
- **Unified input model** — Screenshot, pointer, text input, and execution all use the same trait interface
- **Event audit trail** — Every automation operation is recorded as `AutomationEvent` for playback, debugging, and compliance
- **Distributed orchestration** — Subscribe to dispatch events from Sidekick and automate task execution across platforms

## Install

Add the crate(s) you need:

```bash
cargo add eidolon-core
cargo add eidolon-desktop    # For macOS/Windows/Linux
cargo add eidolon-mobile     # For iOS/Android
cargo add eidolon-sandbox    # For Docker/nanoVMs/KVM
```

Or in your `Cargo.toml`:

```toml
[dependencies]
eidolon-core = { path = "../../eidolon/crates/eidolon-core" }
eidolon-desktop = { path = "../../eidolon/crates/eidolon-desktop" }

tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
phenotype-bus = { path = "../../phenotype-bus" }
```

## Quickstart (20 lines)

```rust
use eidolon_core::{DesktopAutomator, PointerInput, Viewport};
use eidolon_desktop::DesktopClient;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create desktop automator
    let automator = DesktopClient::new()?;

    // Get viewport (screen dimensions)
    let vp: Viewport = automator.get_viewport().await?;
    println!("Screen: {}x{}", vp.width, vp.height);

    // Take screenshot
    automator.screenshot("./before.png").await?;

    // Click at (100, 100)
    automator.pointer(&PointerInput::click(100, 100)).await?;

    // Take another screenshot
    automator.screenshot("./after.png").await?;

    Ok(())
}
```

## Common Patterns

### Pattern 1: Desktop Workflow Automation

Automate UI-based tasks: login, navigate, extract data.

```rust
use eidolon_core::{DesktopAutomator, PointerInput, TextInput, AutomationEvent};

let automator = DesktopClient::new()?;

// Clear old screenshots
automator.screenshot("./step-1-login.png").await?;

// Click login button
automator.pointer(&PointerInput::click(200, 300)).await?;
tokio::time::sleep(std::time::Duration::from_secs(1)).await;

// Type credentials
automator.text(&TextInput::new("user@example.com")).await?;
automator.text(&TextInput::new("password")).await?;

// Submit
automator.pointer(&PointerInput::click(250, 350)).await?;
tokio::time::sleep(std::time::Duration::from_secs(2)).await;

// Verify success
automator.screenshot("./step-2-dashboard.png").await?;

// Record event for audit trail
let event = AutomationEvent {
    id: "login-flow-001".into(),
    event_type: "workflow".into(),
    platform: "desktop".into(),
    payload: serde_json::json!({"steps": 5, "status": "success"}),
    timestamp: std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs(),
};
automator.record_event(event).await?;
```

### Pattern 2: Mobile Test Automation

Automate mobile app testing: tap, swipe, verify.

```rust
use eidolon_core::MobileAutomator;
use eidolon_mobile::MobileClient;

let automator = MobileClient::new("iOS")?;

// Get viewport
let vp = automator.get_viewport().await?;

// Tap button
automator.tap(vp.width / 2, vp.height / 2).await?;

// Swipe up (e.g., scroll)
automator.swipe(100, 300, 100, 100).await?;

// Type in field
automator.input_text("search term").await?;

// Screenshot for verification
automator.screenshot("./result.png").await?;
```

### Pattern 3: Cross-Collection Automation via phenotype-bus

Listen for dispatch events from Sidekick, trigger automation in Eidolon, emit completion events for Observably to trace.

```rust
use phenotype_bus::{Bus, Event};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct DispatchStarted {
    pub dispatch_id: String,
    pub task: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AutomationCompleted {
    pub dispatch_id: String,
    pub screenshots_taken: usize,
}

impl Event for DispatchStarted {
    fn event_name(&self) -> &'static str { "DispatchStarted" }
}

impl Event for AutomationCompleted {
    fn event_name(&self) -> &'static str { "AutomationCompleted" }
}

// Subscribe to Sidekick dispatch events
let dispatch_bus = Bus::<DispatchStarted>::new(100);
let mut rx = dispatch_bus.subscribe();

let automator = DesktopClient::new()?;

// When dispatch starts, automate the task
while let Ok(event) = rx.recv().await {
    automator.screenshot("./before.png").await?;
    automator.pointer(&PointerInput::click(100, 100)).await?;
    automator.screenshot("./after.png").await?;

    // Emit completion event for Observably to trace
    let completion_bus = Bus::<AutomationCompleted>::new(100);
    completion_bus.publish(AutomationCompleted {
        dispatch_id: event.dispatch_id.clone(),
        screenshots_taken: 2,
    }).await?;
}
```

## Cross-Collection Integration

Eidolon integrates via **phenotype-bus**:

- **Subscribes to**: `DispatchStarted` (from Sidekick)
- **Emits**: `AutomationTriggered`, `AutomationCompleted` events
- **Consumed by**: Observably (traces automation), Stashly (stores screenshots)

See [phenotype-bus](../../phenotype-bus/README.md) for event patterns. Eidolon works with [Sidekick](../../Sidekick/README.md) (dispatch triggers automation), [Observably](../../Observably/README.md) (traces execution), [Stashly](../../Stashly/README.md) (caches screenshots), and [Paginary](../../Paginary/README.md) (documents automation workflows).

## Next Steps

- Explore [eidolon-core traits](../crates/eidolon-core/src/traits.rs)
- Read the [DesktopAutomator trait overview](../README.md#desktopautomator)
- Read the [MobileAutomator trait overview](../README.md#mobileautomator)
- Review the [cross-collection integration overview](../README.md#cross-collection-integration)
