//! macOS desktop automation implementation using Core Graphics.

use std::path::Path;
use std::process::Command;

use core_graphics::display::CGDisplay;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use eidolon_core::input::{PointerInput, TextInput};
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};

/// macOS desktop automation client.
/// Does not store CGEventSource because it is not Send+Sync;
/// creates a fresh source per operation instead.
pub struct MacOSClient;

impl MacOSClient {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn create_event_source() -> Result<CGEventSource> {
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState).map_err(|_| {
            eidolon_core::error::PhenoError::Platform("Failed to create CGEventSource".into())
        })
    }

    fn get_main_display_bounds() -> (u32, u32, f64) {
        let display_id = CGDisplay::main();
        let bounds = display_id.bounds();
        let width = bounds.size.width as u32;
        let height = bounds.size.height as u32;
        // TODO: Use NSScreen backingScaleFactor when objc2-app-kit is available
        let dpr = 1.0;
        (width, height, dpr)
    }

    fn cg_point_from_input(input: &PointerInput) -> core_graphics::geometry::CGPoint {
        core_graphics::geometry::CGPoint::new(input.x as f64, input.y as f64)
    }

    fn mouse_button_from_str(button: &Option<String>) -> CGMouseButton {
        match button.as_deref() {
            Some("right") => CGMouseButton::Right,
            Some("middle") => CGMouseButton::Center,
            _ => CGMouseButton::Left,
        }
    }
}

#[async_trait::async_trait]
impl DesktopAutomator for MacOSClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        let (width, height, dpr) = MacOSClient::get_main_display_bounds();
        Ok(Viewport::new(width, height, dpr))
    }

    async fn screenshot(&self, path: &str) -> Result<()> {
        // Validate that the parent directory exists before invoking the platform
        // tool. `screencapture` may return success even when the destination
        // directory is invalid (e.g. it produces a zero-byte file), so we
        // surface that as a `Platform` error up front.
        let path_buf = Path::new(path);
        if let Some(parent) = path_buf.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(eidolon_core::error::PhenoError::Platform(format!(
                    "screenshot parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }

        let output = Command::new("screencapture")
            .args(["-x", "-t", "png", path])
            .output()
            .map_err(|e| {
                eidolon_core::error::PhenoError::Platform(format!(
                    "Failed to launch screencapture: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eidolon_core::error::PhenoError::Platform(format!(
                "screencapture failed: {}",
                stderr
            )));
        }

        log::info!("Screenshot saved to {}", path);
        Ok(())
    }

    async fn pointer(&self, event: &PointerInput) -> Result<()> {
        let point = MacOSClient::cg_point_from_input(event);
        let button = MacOSClient::mouse_button_from_str(&event.button);

        match &event.action {
            eidolon_core::input::PointerAction::Move => {
                let event_source = MacOSClient::create_event_source()?;
                let cg_event =
                    CGEvent::new_mouse_event(event_source, CGEventType::MouseMoved, point, button)
                        .map_err(|_| {
                            eidolon_core::error::PhenoError::Platform(
                                "Failed to create mouse move event".into(),
                            )
                        })?;
                cg_event.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::PointerAction::Click => {
                {
                    let event_source = MacOSClient::create_event_source()?;
                    let down_event = CGEvent::new_mouse_event(
                        event_source,
                        CGEventType::LeftMouseDown,
                        point,
                        button,
                    )
                    .map_err(|_| {
                        eidolon_core::error::PhenoError::Platform(
                            "Failed to create mouse down event".into(),
                        )
                    })?;
                    down_event.post(core_graphics::event::CGEventTapLocation::HID);
                }

                let event_source = MacOSClient::create_event_source()?;
                let up_event =
                    CGEvent::new_mouse_event(event_source, CGEventType::LeftMouseUp, point, button)
                        .map_err(|_| {
                            eidolon_core::error::PhenoError::Platform(
                                "Failed to create mouse up event".into(),
                            )
                        })?;
                up_event.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::PointerAction::DoubleClick => {
                {
                    let event_source = MacOSClient::create_event_source()?;
                    let down_event = CGEvent::new_mouse_event(
                        event_source,
                        CGEventType::LeftMouseDown,
                        point,
                        button,
                    )
                    .map_err(|_| {
                        eidolon_core::error::PhenoError::Platform(
                            "Failed to create mouse down event".into(),
                        )
                    })?;
                    down_event.post(core_graphics::event::CGEventTapLocation::HID);
                }

                let event_source = MacOSClient::create_event_source()?;
                let up_event =
                    CGEvent::new_mouse_event(event_source, CGEventType::LeftMouseUp, point, button)
                        .map_err(|_| {
                            eidolon_core::error::PhenoError::Platform(
                                "Failed to create mouse up event".into(),
                            )
                        })?;
                up_event.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::PointerAction::RightClick => {
                let event_source = MacOSClient::create_event_source()?;
                let cg_event = CGEvent::new_mouse_event(
                    event_source,
                    CGEventType::RightMouseDown,
                    point,
                    button,
                )
                .map_err(|_| {
                    eidolon_core::error::PhenoError::Platform(
                        "Failed to create right mouse down event".into(),
                    )
                })?;
                cg_event.post(core_graphics::event::CGEventTapLocation::HID);

                let event_source = MacOSClient::create_event_source()?;
                let cg_event = CGEvent::new_mouse_event(
                    event_source,
                    CGEventType::RightMouseUp,
                    point,
                    button,
                )
                .map_err(|_| {
                    eidolon_core::error::PhenoError::Platform(
                        "Failed to create right mouse up event".into(),
                    )
                })?;
                cg_event.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::PointerAction::MiddleClick => {
                let event_source = MacOSClient::create_event_source()?;
                let cg_event = CGEvent::new_mouse_event(
                    event_source,
                    CGEventType::LeftMouseDown,
                    point,
                    button,
                )
                .map_err(|_| {
                    eidolon_core::error::PhenoError::Platform(
                        "Failed to create middle mouse down event".into(),
                    )
                })?;
                cg_event.post(core_graphics::event::CGEventTapLocation::HID);

                let event_source = MacOSClient::create_event_source()?;
                let cg_event =
                    CGEvent::new_mouse_event(event_source, CGEventType::LeftMouseUp, point, button)
                        .map_err(|_| {
                            eidolon_core::error::PhenoError::Platform(
                                "Failed to create middle mouse up event".into(),
                            )
                        })?;
                cg_event.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::PointerAction::Scroll => {
                log::warn!("Scroll pointer action not yet implemented on macOS");
            }
        }

        log::debug!(
            "Pointer event executed: ({}, {}) action={:?}",
            event.x,
            event.y,
            event.action
        );
        Ok(())
    }

    async fn text(&self, event: &TextInput) -> Result<()> {
        match &event.input_type {
            eidolon_core::input::InputType::Key => {
                for ch in event.text.chars() {
                    {
                        let event_source = MacOSClient::create_event_source()?;
                        let key_event = CGEvent::new_keyboard_event(event_source, 0, true)
                            .map_err(|_| {
                                eidolon_core::error::PhenoError::Platform(
                                    "Failed to create key event".into(),
                                )
                            })?;
                        key_event.set_string(&ch.to_string());
                        key_event.post(core_graphics::event::CGEventTapLocation::HID);
                    }

                    let delay = event.delay_ms.unwrap_or(10);
                    if delay > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
                    }
                }
            }
            eidolon_core::input::InputType::Paste => {
                let event_source = MacOSClient::create_event_source()?;
                let flags = CGEventFlags::CGEventFlagCommand;
                let key_event = CGEvent::new_keyboard_event(
                    event_source.clone(),
                    9, // 'v' key
                    true,
                )
                .map_err(|_| {
                    eidolon_core::error::PhenoError::Platform("Failed to create paste event".into())
                })?;
                key_event.set_flags(flags);
                key_event.post(core_graphics::event::CGEventTapLocation::HID);

                let key_up = CGEvent::new_keyboard_event(event_source, 9, false).map_err(|_| {
                    eidolon_core::error::PhenoError::Platform(
                        "Failed to create paste up event".into(),
                    )
                })?;
                key_up.set_flags(flags);
                key_up.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::InputType::Char => {
                let event_source = MacOSClient::create_event_source()?;
                let select_all = CGEvent::new_keyboard_event(event_source.clone(), 0, true)
                    .map_err(|_| {
                        eidolon_core::error::PhenoError::Platform(
                            "Failed to create select all event".into(),
                        )
                    })?;
                select_all.set_flags(CGEventFlags::CGEventFlagCommand);
                select_all.post(core_graphics::event::CGEventTapLocation::HID);

                let delete = CGEvent::new_keyboard_event(
                    event_source,
                    51, // Delete key
                    true,
                )
                .map_err(|_| {
                    eidolon_core::error::PhenoError::Platform(
                        "Failed to create delete event".into(),
                    )
                })?;
                delete.post(core_graphics::event::CGEventTapLocation::HID);
            }
            eidolon_core::input::InputType::IME => {
                log::warn!("IME input type not yet implemented on macOS");
            }
        }

        log::debug!(
            "Text input executed: type={:?} text={}",
            event.input_type,
            event.text
        );
        Ok(())
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event: {:?}", event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use eidolon_core::input::PointerInput;

    use super::*;

    #[test]
    fn mouse_button_from_str_left() {
        let button = MacOSClient::mouse_button_from_str(&Some("left".to_string()));
        assert!(matches!(button, CGMouseButton::Left));
    }

    #[test]
    fn mouse_button_from_str_right() {
        let button = MacOSClient::mouse_button_from_str(&Some("right".to_string()));
        assert!(matches!(button, CGMouseButton::Right));
    }

    #[test]
    fn mouse_button_from_str_middle() {
        let button = MacOSClient::mouse_button_from_str(&Some("middle".to_string()));
        assert!(matches!(button, CGMouseButton::Center));
    }

    #[test]
    fn mouse_button_from_str_none() {
        let button = MacOSClient::mouse_button_from_str(&None);
        assert!(matches!(button, CGMouseButton::Left));
    }

    #[test]
    fn cg_point_from_input_valid() {
        let input = PointerInput::click(100, 200);
        let point = MacOSClient::cg_point_from_input(&input);
        assert_eq!(point.x, 100.0);
        assert_eq!(point.y, 200.0);
    }

    #[test]
    fn cg_point_from_input_negative_coords() {
        let input = PointerInput::move_to(-50, -100);
        let point = MacOSClient::cg_point_from_input(&input);
        assert_eq!(point.x, -50.0);
        assert_eq!(point.y, -100.0);
    }

    #[test]
    fn cg_point_from_input_zero_origin() {
        let input = PointerInput::click(0, 0);
        let point = MacOSClient::cg_point_from_input(&input);
        assert_eq!(point.x, 0.0);
        assert_eq!(point.y, 0.0);
    }

    #[test]
    fn mouse_button_from_str_unrecognized_defaults_left() {
        let button = MacOSClient::mouse_button_from_str(&Some("unknown".to_string()));
        assert!(matches!(button, CGMouseButton::Left));
    }

    #[test]
    fn mouse_button_from_str_empty_string() {
        let button = MacOSClient::mouse_button_from_str(&Some(String::new()));
        assert!(matches!(button, CGMouseButton::Left));
    }

    #[test]
    fn macos_client_new_succeeds() {
        let client = MacOSClient::new().expect("new should succeed");
        drop(client);
    }

    #[test]
    fn create_event_source_succeeds() {
        let _source =
            MacOSClient::create_event_source().expect("CGEventSource creation should succeed");
    }

    #[test]
    fn get_main_display_bounds_returns_positive_dimensions() {
        let (width, height, _dpr) = MacOSClient::get_main_display_bounds();
        assert!(width > 0, "width should be positive, got {width}");
        assert!(height > 0, "height should be positive, got {height}");
    }

    #[tokio::test]
    async fn get_viewport_returns_nonzero_dimensions() {
        let client = MacOSClient::new().unwrap();
        let viewport = client.get_viewport().await.unwrap();
        assert!(viewport.width > 0);
        assert!(viewport.height > 0);
    }

    #[tokio::test]
    async fn record_event_succeeds() {
        use eidolon_core::event::{AutomationEvent, EventPayload, EventType, Platform};
        use eidolon_core::input::{PointerAction, PointerInput};

        let client = MacOSClient::new().unwrap();
        let event = AutomationEvent {
            id: "test-id".into(),
            event_type: EventType::MouseMove,
            platform: Platform::MacOS,
            payload: EventPayload::Pointer(PointerInput {
                x: 10,
                y: 20,
                button: None,
                action: PointerAction::Move,
                duration_ms: None,
            }),
            timestamp: 0,
        };
        let result = client.record_event(event).await;
        assert!(result.is_ok());
    }

    #[test]
    fn desktop_automator_trait_is_implemented() {
        fn assert_automator<T: eidolon_core::traits::DesktopAutomator>() {}
        assert_automator::<MacOSClient>();
    }
}
