//! Pure Wayland pointer/keyboard inject via RemoteDesktop + Screencast portals.
//!
//! wraps: ashpd 0.11 — `org.freedesktop.portal.RemoteDesktop` +
//! `org.freedesktop.portal.ScreenCast` (absolute pointer needs a PipeWire
//! stream node id from Screencast on the same session).
//!
//! # Fail-loud codes
//!
//! | Condition | Code |
//! |---|---|
//! | Portal / session bus missing, user cancel, start failure | [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`] |
//! | Session started but Pointer/Keyboard not granted, or no Screencast stream for absolute motion | [`codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`] |
//!
//! Destructive callers must already gate with `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
//!
//! # Honesty gaps (not claimed)
//!
//! - Without `EIDOLON_DESKTOP_WAYLAND_RESTORE=1`, one portal session per inject
//!   call (desktop may prompt each time).
//! - With restore enabled, a stored token skips re-prompt when the portal
//!   accepts it; stale/revoked tokens still require interactive grant.
//! - `paste` / `clear` send Ctrl+V / Ctrl+A+Delete keysyms only — they do not
//!   put text on the clipboard (same contract as the X11 path).
//! - Keystroke Latin-1 / Return / Tab only (same as X11 keysym map).

use crate::codes;
use crate::linux::wayland::map_portal_err;
use crate::linux::wayland_restore_token::{self, RestoreConfig};
use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::{PersistMode, Session};
use eidolon_core::error::PhenoError;
use eidolon_core::input::{PointerInput, TextInput};
use eidolon_core::Result;

/// Linux `input-event-codes.h` button constants (portal `NotifyPointerButton`).
const BTN_LEFT: i32 = 0x110;
const BTN_RIGHT: i32 = 0x111;
const BTN_MIDDLE: i32 = 0x112;

/// X11 keysyms used for modifiers / editing (portal `NotifyKeyboardKeysym`).
const XK_CONTROL_L: i32 = 0xffe3;
const XK_DELETE: i32 = 0xffff;
const XK_A: i32 = 0x0061;
const XK_V: i32 = 0x0076;

fn input_gap(method: &str, detail: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED,
        format!(
            "LinuxDesktopDriver::{method} on pure Wayland — {detail}; \
             see docs/EXTRACTION_PLAN.md"
        ),
    )
}

pub(crate) fn button_code(button: &Option<String>) -> i32 {
    match button.as_deref() {
        Some("middle") => BTN_MIDDLE,
        Some("right") => BTN_RIGHT,
        _ => BTN_LEFT,
    }
}

pub(crate) fn keysym_for_char(ch: char) -> Result<i32> {
    let cp = ch as u32;
    // Latin-1 printable maps 1:1 to X11 keysyms (portal keysym path).
    if (0x20..=0xff).contains(&cp) {
        return Ok(cp as i32);
    }
    if ch == '\n' || ch == '\r' {
        return Ok(0xff0d); // XK_Return
    }
    if ch == '\t' {
        return Ok(0xff09); // XK_Tab
    }
    Err(PhenoError::Platform(format!(
        "Wayland keystroke unsupported for non-Latin-1 char U+{cp:04X} — \
         use ASCII or paste via clipboard+Ctrl+V"
    )))
}

async fn select_devices(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    need_pointer: bool,
    need_keyboard: bool,
    restore: &RestoreConfig,
) -> Result<()> {
    let types = match (need_pointer, need_keyboard) {
        (true, true) | (false, false) => DeviceType::Pointer | DeviceType::Keyboard,
        (true, false) => DeviceType::Pointer.into(),
        (false, true) => DeviceType::Keyboard.into(),
    };
    let persist_mode = if restore.enabled {
        PersistMode::ExplicitlyRevoked
    } else {
        PersistMode::DoNot
    };
    remote
        .select_devices(
            session,
            types,
            restore.token.as_deref(),
            persist_mode,
        )
        .await
        .map_err(|e| map_portal_err("RemoteDesktop SelectDevices failed", e))?;
    Ok(())
}

async fn tap_keysym(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    keysym: i32,
) -> Result<()> {
    remote
        .notify_keyboard_keysym(session, keysym, KeyState::Pressed)
        .await
        .map_err(|e| map_portal_err("NotifyKeyboardKeysym press failed", e))?;
    remote
        .notify_keyboard_keysym(session, keysym, KeyState::Released)
        .await
        .map_err(|e| map_portal_err("NotifyKeyboardKeysym release failed", e))?;
    Ok(())
}

async fn chord_ctrl(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    keysym: i32,
) -> Result<()> {
    remote
        .notify_keyboard_keysym(session, XK_CONTROL_L, KeyState::Pressed)
        .await
        .map_err(|e| map_portal_err("NotifyKeyboardKeysym Ctrl press failed", e))?;
    remote
        .notify_keyboard_keysym(session, keysym, KeyState::Pressed)
        .await
        .map_err(|e| map_portal_err("NotifyKeyboardKeysym chord press failed", e))?;
    remote
        .notify_keyboard_keysym(session, keysym, KeyState::Released)
        .await
        .map_err(|e| map_portal_err("NotifyKeyboardKeysym chord release failed", e))?;
    remote
        .notify_keyboard_keysym(session, XK_CONTROL_L, KeyState::Released)
        .await
        .map_err(|e| map_portal_err("NotifyKeyboardKeysym Ctrl release failed", e))?;
    Ok(())
}

async fn move_absolute(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    stream_id: u32,
    x: i32,
    y: i32,
) -> Result<()> {
    remote
        .notify_pointer_motion_absolute(session, stream_id, f64::from(x), f64::from(y))
        .await
        .map_err(|e| map_portal_err("NotifyPointerMotionAbsolute failed", e))
}

async fn button(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    code: i32,
    state: KeyState,
) -> Result<()> {
    remote
        .notify_pointer_button(session, code, state)
        .await
        .map_err(|e| map_portal_err("NotifyPointerButton failed", e))
}

async fn inject_pointer(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    stream_id: u32,
    event: &PointerInput,
) -> Result<()> {
    let btn = button_code(&event.button);
    match event.action.as_str() {
        "move" => {
            move_absolute(remote, session, stream_id, event.x, event.y).await?;
        }
        "press" | "tap" => {
            move_absolute(remote, session, stream_id, event.x, event.y).await?;
            button(remote, session, btn, KeyState::Pressed).await?;
            if event.action == "tap" {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            button(remote, session, btn, KeyState::Released).await?;
        }
        "release" => {
            button(remote, session, btn, KeyState::Released).await?;
        }
        "long_press" => {
            move_absolute(remote, session, stream_id, event.x, event.y).await?;
            button(remote, session, btn, KeyState::Pressed).await?;
            let duration = event.duration_ms.unwrap_or(500);
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(duration))).await;
            button(remote, session, btn, KeyState::Released).await?;
        }
        "drag" => {
            button(remote, session, btn, KeyState::Pressed).await?;
            move_absolute(remote, session, stream_id, event.x, event.y).await?;
            button(remote, session, btn, KeyState::Released).await?;
        }
        other => {
            log::warn!("Unknown pointer action (Wayland): {other}");
        }
    }
    log::debug!(
        "Pointer event executed (Wayland RemoteDesktop): ({}, {}) action={}",
        event.x,
        event.y,
        event.action
    );
    Ok(())
}

async fn inject_text(
    remote: &RemoteDesktop<'_>,
    session: &Session<'_, RemoteDesktop<'_>>,
    event: &TextInput,
) -> Result<()> {
    let delay = event.delay_ms.unwrap_or(10);
    match event.input_type.as_str() {
        "keystroke" => {
            for ch in event.text.chars() {
                let keysym = keysym_for_char(ch)?;
                tap_keysym(remote, session, keysym).await?;
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(u64::from(delay))).await;
                }
            }
        }
        "paste" => {
            chord_ctrl(remote, session, XK_V).await?;
        }
        "clear" => {
            chord_ctrl(remote, session, XK_A).await?;
            tap_keysym(remote, session, XK_DELETE).await?;
        }
        other => {
            log::warn!("Unknown text input type (Wayland): {other}");
        }
    }
    log::debug!(
        "Text input executed (Wayland RemoteDesktop): type={} text={}",
        event.input_type,
        event.text
    );
    Ok(())
}

/// Shared RemoteDesktop (+ optional Screencast) session setup + inject + close.
async fn run_session(
    need_pointer: bool,
    need_keyboard: bool,
    pointer: Option<&PointerInput>,
    text: Option<&TextInput>,
) -> Result<()> {
    let restore = RestoreConfig::load()?;

    let remote = RemoteDesktop::new()
        .await
        .map_err(|e| map_portal_err("RemoteDesktop portal connect failed", e))?;

    let session = remote
        .create_session()
        .await
        .map_err(|e| map_portal_err("RemoteDesktop CreateSession failed", e))?;

    if let Err(e) =
        select_devices(&remote, &session, need_pointer, need_keyboard, &restore).await
    {
        let _ = session.close().await;
        return Err(e);
    }

    // Absolute pointer motion requires a Screencast stream node id on the
    // same session (ashpd docs / xdg-desktop-portal RemoteDesktop).
    if need_pointer {
        let screencast = Screencast::new()
            .await
            .map_err(|e| map_portal_err("ScreenCast portal connect failed", e));
        let screencast = match screencast {
            Ok(s) => s,
            Err(e) => {
                let _ = session.close().await;
                return Err(e);
            }
        };
        if let Err(e) = screencast
            .select_sources(
                &session,
                CursorMode::Metadata,
                SourceType::Monitor.into(),
                false,
                restore.token.as_deref(),
                if restore.enabled {
                    PersistMode::ExplicitlyRevoked
                } else {
                    PersistMode::DoNot
                },
            )
            .await
            .map_err(|err| map_portal_err("ScreenCast SelectSources failed", err))
        {
            let _ = session.close().await;
            return Err(e);
        }
    }

    let selected = match remote
        .start(&session, None)
        .await
        .map_err(|e| map_portal_err("RemoteDesktop Start failed", e))
    {
        Ok(req) => match req
            .response()
            .map_err(|e| map_portal_err("RemoteDesktop Start response failed", e))
        {
            Ok(s) => s,
            Err(e) => {
                let _ = session.close().await;
                return Err(e);
            }
        },
        Err(e) => {
            let _ = session.close().await;
            return Err(e);
        }
    };

    let granted = selected.devices();
    if need_pointer && !granted.contains(DeviceType::Pointer) {
        let _ = session.close().await;
        return Err(input_gap(
            "pointer",
            "RemoteDesktop session started but Pointer device was not granted",
        ));
    }
    if need_keyboard && !granted.contains(DeviceType::Keyboard) {
        let _ = session.close().await;
        return Err(input_gap(
            "text",
            "RemoteDesktop session started but Keyboard device was not granted",
        ));
    }

    let stream_id = if need_pointer {
        match selected
            .streams()
            .and_then(|streams| streams.first())
            .map(|s| s.pipe_wire_node_id())
        {
            Some(id) => id,
            None => {
                let _ = session.close().await;
                return Err(input_gap(
                    "pointer",
                    "RemoteDesktop+Screencast session has no PipeWire stream \
                     (absolute NotifyPointerMotionAbsolute requires a stream \
                     node id)",
                ));
            }
        }
    } else {
        0
    };

    let result = async {
        if let Some(event) = pointer {
            inject_pointer(&remote, &session, stream_id, event).await?;
        }
        if let Some(event) = text {
            inject_text(&remote, &session, event).await?;
        }
        Ok(())
    }
    .await;

    if restore.enabled {
        if let Some(new_token) = selected.restore_token() {
            wayland_restore_token::save_token(&restore.path, new_token)?;
        }
    }

    let _ = session.close().await;
    result
}

/// Absolute pointer inject via RemoteDesktop + Screencast.
pub async fn pointer(event: &PointerInput) -> Result<()> {
    run_session(true, false, Some(event), None).await
}

/// Keyboard / text inject via RemoteDesktop keysyms.
pub async fn text(event: &TextInput) -> Result<()> {
    run_session(false, true, None, Some(event)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn button_code_maps_left_right_middle() {
        assert_eq!(button_code(&None), BTN_LEFT);
        assert_eq!(button_code(&Some("left".into())), BTN_LEFT);
        assert_eq!(button_code(&Some("right".into())), BTN_RIGHT);
        assert_eq!(button_code(&Some("middle".into())), BTN_MIDDLE);
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn keysym_for_char_latin1_and_controls() {
        assert_eq!(keysym_for_char('a').unwrap(), 0x61);
        assert_eq!(keysym_for_char('A').unwrap(), 0x41);
        assert_eq!(keysym_for_char('\n').unwrap(), 0xff0d);
        assert_eq!(keysym_for_char('\t').unwrap(), 0xff09);
        assert!(keysym_for_char('😀').is_err());
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn input_gap_uses_stable_code() {
        let err = input_gap("pointer", "test");
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED)
        );
    }
}
