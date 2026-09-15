//! Unit tests for eidolon-core.

use eidolon_core::{
    AutomationEvent, EventType, InputType, Platform, PointerAction, PointerInput, TextInput,
    Viewport,
};

mod viewport {
    use super::*;

    #[test]
    fn viewport_new_landscape() {
        let v = Viewport::new(1920, 1080, 1.0);
        assert_eq!(v.width, 1920);
        assert_eq!(v.height, 1080);
        assert_eq!(v.dpr, 1.0);
        assert_eq!(v.orientation, "landscape");
    }

    #[test]
    fn viewport_new_portrait() {
        let v = Viewport::new(1080, 1920, 2.0);
        assert_eq!(v.width, 1080);
        assert_eq!(v.height, 1920);
        assert_eq!(v.dpr, 2.0);
        assert_eq!(v.orientation, "portrait");
    }

    #[test]
    fn viewport_new_square() {
        let v = Viewport::new(1080, 1080, 1.5);
        assert_eq!(v.orientation, "portrait"); // width == height uses portrait
    }

    #[test]
    fn viewport_desktop_fhd() {
        let v = Viewport::desktop_fhd();
        assert_eq!(v.width, 1920);
        assert_eq!(v.height, 1080);
        assert_eq!(v.dpr, 1.0);
        assert_eq!(v.orientation, "landscape");
    }

    #[test]
    fn viewport_mobile_fhd() {
        let v = Viewport::mobile_fhd();
        assert_eq!(v.width, 1080);
        assert_eq!(v.height, 1920);
        assert_eq!(v.dpr, 2.0);
        assert_eq!(v.orientation, "portrait");
    }

    #[test]
    fn viewport_tablet_qhd() {
        let v = Viewport::tablet_qhd();
        assert_eq!(v.width, 2560);
        assert_eq!(v.height, 1440);
        assert_eq!(v.dpr, 1.5);
        assert_eq!(v.orientation, "landscape");
    }

    #[test]
    fn viewport_clone() {
        let v1 = Viewport::desktop_fhd();
        let v2 = v1.clone();
        assert_eq!(v1.width, v2.width);
        assert_eq!(v1.height, v2.height);
        assert_eq!(v1.dpr, v2.dpr);
        assert_eq!(v1.orientation, v2.orientation);
    }

    #[test]
    fn viewport_debug() {
        let v = Viewport::desktop_fhd();
        let debug = format!("{:?}", v);
        assert!(debug.contains("1920"));
        assert!(debug.contains("1080"));
    }
}

mod pointer_input {
    use super::*;

    #[test]
    fn pointer_input_click() {
        let p = PointerInput::click(100, 200);
        assert_eq!(p.x, 100);
        assert_eq!(p.y, 200);
        assert_eq!(p.button, Some("left".to_string()));
        assert_eq!(p.action, PointerAction::Click);
        assert!(p.duration_ms.is_none());
    }

    #[test]
    fn pointer_input_move_to() {
        let p = PointerInput::move_to(300, 400);
        assert_eq!(p.x, 300);
        assert_eq!(p.y, 400);
        assert!(p.button.is_none());
        assert_eq!(p.action, PointerAction::Move);
        assert!(p.duration_ms.is_none());
    }

    #[test]
    fn pointer_input_with_duration() {
        let p = PointerInput {
            x: 50,
            y: 75,
            button: Some("right".to_string()),
            action: PointerAction::Click,
            duration_ms: Some(500),
        };
        assert_eq!(p.x, 50);
        assert_eq!(p.y, 75);
        assert_eq!(p.button, Some("right".to_string()));
        assert_eq!(p.action, PointerAction::Click);
        assert_eq!(p.duration_ms, Some(500));
    }

    #[test]
    fn pointer_input_clone() {
        let p1 = PointerInput::click(10, 20);
        let p2 = p1.clone();
        assert_eq!(p1.x, p2.x);
        assert_eq!(p1.y, p2.y);
    }

    #[test]
    fn pointer_input_debug() {
        let p = PointerInput::click(99, 88);
        let debug = format!("{:?}", p);
        assert!(debug.contains("99"));
        assert!(debug.contains("88"));
    }

    #[test]
    fn pointer_input_serialize() {
        let p = PointerInput::click(100, 200);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("100"));
        assert!(json.contains("200"));
        assert!(json.contains("left"));
    }

    #[test]
    fn pointer_input_deserialize() {
        let json = r#"{"x":150,"y":250,"button":"right","action":"click","duration_ms":null}"#;
        let p: PointerInput = serde_json::from_str(json).unwrap();
        assert_eq!(p.x, 150);
        assert_eq!(p.y, 250);
        assert_eq!(p.button, Some("right".to_string()));
    }
}

mod text_input {
    use super::*;

    #[test]
    fn text_input_keystroke() {
        let t = TextInput::keystroke("hello");
        assert_eq!(t.text, "hello");
        assert_eq!(t.input_type, InputType::Key);
        assert!(t.delay_ms.is_none());
    }

    #[test]
    fn text_input_paste() {
        let t = TextInput::paste("pasted text");
        assert_eq!(t.text, "pasted text");
        assert_eq!(t.input_type, InputType::Paste);
        assert!(t.delay_ms.is_none());
    }

    #[test]
    fn text_input_with_delay() {
        let t = TextInput {
            text: "typed".to_string(),
            input_type: InputType::Key,
            delay_ms: Some(50),
        };
        assert_eq!(t.delay_ms, Some(50));
    }

    #[test]
    fn text_input_clone() {
        let t1 = TextInput::keystroke("clone test");
        let t2 = t1.clone();
        assert_eq!(t1.text, t2.text);
        assert_eq!(t1.input_type, t2.input_type);
    }

    #[test]
    fn text_input_debug() {
        let t = TextInput::paste("debug content");
        let debug = format!("{:?}", t);
        assert!(debug.contains("debug content"));
    }

    #[test]
    fn text_input_serialize() {
        let t = TextInput::keystroke("test");
        let json = serde_json::to_string(&t).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["text"], "test");
        assert_eq!(parsed["input_type"], "key");
    }

    #[test]
    fn text_input_deserialize() {
        let json = r#"{"text":"deserialized","input_type":"paste","delay_ms":100}"#;
        let t: TextInput = serde_json::from_str(json).unwrap();
        assert_eq!(t.text, "deserialized");
        assert_eq!(t.input_type, InputType::Paste);
        assert_eq!(t.delay_ms, Some(100));
    }
}

mod automation_event {
    use super::*;

    #[test]
    fn automation_event_pointer() {
        let input = PointerInput::click(100, 200);
        let event = AutomationEvent::pointer(Platform::Unknown, input);
        assert_eq!(event.event_type, EventType::Custom("pointer".into()));
        assert_eq!(event.platform, Platform::Unknown);
        assert!(!event.id.is_empty());
        assert!(event.timestamp > 0);
    }

    #[test]
    fn automation_event_text() {
        let input = TextInput::keystroke("hello world");
        let event = AutomationEvent::text(Platform::Unknown, input);
        assert_eq!(event.event_type, EventType::Custom("text".into()));
        assert_eq!(event.platform, Platform::Unknown);
        assert!(!event.id.is_empty());
    }

    #[test]
    fn automation_event_screenshot() {
        let event = AutomationEvent::screenshot(Platform::Unknown, "/tmp/screen.png");
        assert_eq!(event.event_type, EventType::Custom("screenshot".into()));
        assert_eq!(event.platform, Platform::Unknown);
        match event.payload {
            eidolon_core::event::EventPayload::Screenshot { ref path } => {
                assert_eq!(path, "/tmp/screen.png");
            }
            _ => panic!("Expected Screenshot payload"),
        }
    }

    #[test]
    fn automation_event_unique_ids() {
        let e1 = AutomationEvent::screenshot(Platform::Unknown, "/a.png");
        let e2 = AutomationEvent::screenshot(Platform::Unknown, "/b.png");
        assert_ne!(e1.id, e2.id);
    }

    #[test]
    fn automation_event_clone() {
        let e1 = AutomationEvent::pointer(Platform::Linux, PointerInput::click(1, 2));
        let e2 = e1.clone();
        assert_eq!(e1.id, e2.id);
        assert_eq!(e1.event_type, e2.event_type);
        assert_eq!(e1.platform, e2.platform);
    }

    #[test]
    fn automation_event_serialize() {
        let event = AutomationEvent::screenshot(Platform::Unknown, "/img.png");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("screenshot"));
        assert!(json.contains("/img.png"));
    }

    #[test]
    fn automation_event_deserialize() {
        let json = r#"{"id":"test-id","event_type":{"custom":"pointer"},"platform":"android","payload":{"x":10,"y":20,"button":"left","action":"click","duration_ms":null},"timestamp":1700000000}"#;
        let event: AutomationEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "test-id");
        assert_eq!(event.event_type, EventType::Custom("pointer".into()));
        assert_eq!(event.platform, Platform::Android);
    }

    #[test]
    fn automation_event_custom_payload() {
        let json = r#"{"id":"cust-1","event_type":{"custom":"test_event"},"platform":"unknown","payload":{"data":{"foo":"bar"}},"timestamp":1700000000}"#;
        let event: AutomationEvent = serde_json::from_str(json).unwrap();
        match event.payload {
            eidolon_core::event::EventPayload::Custom { .. } => {}
            _ => panic!("Expected Custom payload"),
        }
    }
}

mod version {
    use eidolon_core::VERSION;

    #[test]
    fn version_defined() {
        assert_eq!(VERSION, "0.0.1");
    }
}
mod virtual_stage_tests {
    //! Unit tests for the `VirtualStage` trait surface and its
    //! `MobileStage` / `SandboxStage` sub-traits.
    //!
    //! These tests cover the three concerns called out in
    //! `plans/2026-06-09-eidolon-platform-impl-plan-v1.md` §3.1:
    //!
    //! 1. The trait is *implementable* for a zero-sized (unit) type —
    //!    i.e. there's no hidden `Sized` bound or lifetime leak that
    //!    would block a real platform impl.
    //! 2. The `MobileStage` default impls (`tap` / `swipe` /
    //!    `input_text`) return `Ok(())` (the documented "empty value"
    //!    contract) for all input shapes.
    //! 3. The `SandboxStage` default impls (`get_metadata` / `start` /
    //!    `stop` / `exec` / `resource_usage`) return `Ok(())` (or the
    //!    documented `SandboxMetadata` / `ResourceUsage` defaults).

    use async_trait::async_trait;
    use eidolon_core::traits::{ResourceUsage, SandboxMetadata};
    use eidolon_core::{MobileStage, SandboxStage, VirtualStage};

    use super::*;

    /// A unit struct that implements `VirtualStage`.
    ///
    /// The fact that this `impl` block compiles — and the struct
    /// stays zero-sized — is the load-bearing assertion that
    /// `VirtualStage` is implementable for a `()`-shaped type.
    struct UnitStage;

    #[async_trait]
    impl VirtualStage for UnitStage {
        async fn get_viewport(&self) -> eidolon_core::Result<Viewport> {
            Ok(Viewport::desktop_fhd())
        }
        async fn screenshot(&self, _path: &str) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn pointer(&self, _event: &PointerInput) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn text(&self, _event: &TextInput) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn record_event(&self, _event: AutomationEvent) -> eidolon_core::Result<()> {
            Ok(())
        }
    }

    /// A unit struct that implements `VirtualStage + MobileStage`.
    ///
    /// Used to exercise the `MobileStage` default impls (`tap`,
    /// `swipe`, `input_text`) without overriding any of them.
    struct MobileOnlyStage;

    #[async_trait]
    impl VirtualStage for MobileOnlyStage {
        async fn get_viewport(&self) -> eidolon_core::Result<Viewport> {
            Ok(Viewport::mobile_fhd())
        }
        async fn screenshot(&self, _path: &str) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn pointer(&self, _event: &PointerInput) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn text(&self, _event: &TextInput) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn record_event(&self, _event: AutomationEvent) -> eidolon_core::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl MobileStage for MobileOnlyStage {
        // No method overrides — exercise the default no-op `Ok(())`
        // impls for `tap` / `swipe` / `input_text` below.
    }

    /// A unit struct that implements `VirtualStage + SandboxStage`.
    ///
    /// Used to exercise the `SandboxStage` default impls
    /// (`get_metadata`, `start`, `stop`, `exec`, `resource_usage`)
    /// without overriding any of them.
    struct SandboxOnlyStage;

    #[async_trait]
    impl VirtualStage for SandboxOnlyStage {
        async fn get_viewport(&self) -> eidolon_core::Result<Viewport> {
            Ok(Viewport::desktop_fhd())
        }
        async fn screenshot(&self, _path: &str) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn pointer(&self, _event: &PointerInput) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn text(&self, _event: &TextInput) -> eidolon_core::Result<()> {
            Ok(())
        }
        async fn record_event(&self, _event: AutomationEvent) -> eidolon_core::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl SandboxStage for SandboxOnlyStage {
        // No method overrides — exercise the default impls below.
    }

    /// (a) The trait compiles for a unit struct, the struct remains
    /// zero-sized, and the required `VirtualStage` methods are
    /// callable through `&dyn VirtualStage` (object-safety / dispatch
    /// smoke check).
    #[tokio::test]
    async fn virtual_stage_compiles_for_unit_struct() {
        // Compilation barrier: this whole module compiles only if
        // `VirtualStage` is implementable for a unit struct.
        let stage = UnitStage;
        assert_eq!(std::mem::size_of::<UnitStage>(), 0);

        // Dispatch through a trait object confirms the trait is
        // object-safe (`Send + Sync` super-traits + `async_trait`
        // codegen are wired correctly).
        let dyn_ref: &dyn VirtualStage = &stage;
        let viewport = dyn_ref.get_viewport().await.unwrap();
        assert_eq!(viewport.width, 1920);
        assert_eq!(viewport.height, 1080);
        assert_eq!(viewport.orientation, "landscape");

        assert!(dyn_ref.screenshot("/tmp/unit.png").await.is_ok());
        assert!(dyn_ref.pointer(&PointerInput::click(10, 20)).await.is_ok());
        assert!(dyn_ref.text(&TextInput::keystroke("hi")).await.is_ok());
        assert!(dyn_ref
            .record_event(AutomationEvent::screenshot(Platform::Unknown, "/u.png"))
            .await
            .is_ok());
    }

    /// (b) Default impls on `MobileStage` (`tap`, `swipe`,
    /// `input_text`) return `Ok(())` — the documented "empty value"
    /// contract — for all input shapes (boundary coordinates, empty
    /// strings, unicode / emoji).
    #[tokio::test]
    async fn mobile_stage_default_impls_return_ok() {
        let stage = MobileOnlyStage;

        // Required-method smoke check (confirms our impl compiles
        // and the trait is wired through the sub-trait).
        let viewport = stage.get_viewport().await.unwrap();
        assert_eq!(viewport.width, 1080);
        assert_eq!(viewport.height, 1920);
        assert!(stage.screenshot("/tmp/m.png").await.is_ok());

        // `tap` default impl returns `Ok(())` (empty value) for
        // any `(x, y)` — interior, origin, and boundary coords.
        assert!(stage.tap(100, 200).await.is_ok());
        assert!(stage.tap(0, 0).await.is_ok());
        assert!(stage.tap(i32::MAX, i32::MIN).await.is_ok());
        assert!(stage.tap(-1, -1).await.is_ok());

        // `swipe` default impl returns `Ok(())` for any rectangle
        // — zero-length, negative coords, large coords.
        assert!(stage.swipe(0, 0, 100, 200).await.is_ok());
        assert!(stage.swipe(1, 2, 3, 4).await.is_ok());
        assert!(stage.swipe(-100, -200, 100, 200).await.is_ok());
        assert!(stage.swipe(0, 0, 0, 0).await.is_ok());

        // `input_text` default impl returns `Ok(())` for any text
        // — including empty and unicode / emoji payloads.
        assert!(stage.input_text("hello").await.is_ok());
        assert!(stage.input_text("").await.is_ok());
        assert!(stage.input_text("🦀 emoji 🦀").await.is_ok());
        assert!(stage.input_text("\n\t\r").await.is_ok());
    }

    /// (c) Default impls on `SandboxStage` return `Ok(())` and the
    /// documented `SandboxMetadata` / `ResourceUsage` defaults.
    #[tokio::test]
    async fn sandbox_stage_default_impls_return_ok_and_default_metadata() {
        let stage = SandboxOnlyStage;

        // Required-method smoke check.
        let viewport = stage.get_viewport().await.unwrap();
        assert_eq!(viewport.width, 1920);
        assert!(stage.screenshot("/tmp/s.png").await.is_ok());

        // `get_metadata` returns the documented default
        // `SandboxMetadata`.
        let meta: SandboxMetadata = stage.get_metadata().await.unwrap();
        assert_eq!(meta.id, "virtual-stage");
        assert_eq!(meta.image, "n/a");
        assert_eq!(meta.cpu_limit, 0);
        assert_eq!(meta.memory_limit_mb, 0);
        assert_eq!(meta.disk_limit_mb, None);

        // `start` / `stop` default impls return `Ok(())` and are
        // idempotent under repeated calls.
        assert!(stage.start().await.is_ok());
        assert!(stage.start().await.is_ok());
        assert!(stage.stop().await.is_ok());
        assert!(stage.stop().await.is_ok());

        // `exec` default impl returns `Ok(String::new())` (the
        // "empty value" contract) for any command, including the
        // empty string.
        let out = stage.exec("ls -la").await.unwrap();
        assert_eq!(out, "");
        let out_empty = stage.exec("").await.unwrap();
        assert_eq!(out_empty, "");

        // `resource_usage` default impl returns the zero-valued
        // `ResourceUsage`.
        let usage: ResourceUsage = stage.resource_usage().await.unwrap();
        assert_eq!(usage.cpu_percent, 0.0);
        assert_eq!(usage.memory_mb, 0);
        assert_eq!(usage.disk_mb, None);
    }
}
