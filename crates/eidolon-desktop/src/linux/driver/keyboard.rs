//! X11 keyboard injection helpers (keysym mapping, tap, chord, text).

use std::collections::HashMap;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::rust_connection::RustConnection;

use super::{fake_input, root_window};

pub(super) const XK_SHIFT_L: u32 = 0xffe1;
pub(super) const XK_CONTROL_L: u32 = 0xffe3;

pub(super) fn build_keysym_map(conn: &RustConnection) -> Result<HashMap<u32, (u8, bool)>> {
    let setup = conn.setup();
    let min_kc = setup.min_keycode;
    let max_kc = setup.max_keycode;
    let count = max_kc.saturating_sub(min_kc).saturating_add(1);
    let reply = conn
        .get_keyboard_mapping(min_kc, count)
        .map_err(|e| PhenoError::Platform(format!("GetKeyboardMapping: {e}")))?
        .reply()
        .map_err(|e| PhenoError::Platform(format!("GetKeyboardMapping reply: {e}")))?;
    let per = reply.keysyms_per_keycode as usize;
    if per == 0 {
        return Err(PhenoError::Platform(
            "GetKeyboardMapping returned 0 keysyms_per_keycode".into(),
        ));
    }
    let mut map = HashMap::new();
    for (i, chunk) in reply.keysyms.chunks(per).enumerate() {
        let keycode = min_kc.saturating_add(i as u8);
        if let Some(&base) = chunk.first() {
            if base != 0 {
                map.entry(base).or_insert((keycode, false));
            }
        }
        if per > 1 {
            if let Some(&shifted) = chunk.get(1) {
                if shifted != 0 {
                    map.entry(shifted).or_insert((keycode, true));
                }
            }
        }
    }
    Ok(map)
}

pub(super) fn keysym_for_char(ch: char) -> Result<u32> {
    let cp = ch as u32;
    if (0x20..=0xff).contains(&cp) {
        return Ok(cp);
    }
    if ch == '\n' || ch == '\r' {
        return Ok(0xff0d);
    }
    if ch == '\t' {
        return Ok(0xff09);
    }
    Err(PhenoError::Platform(format!(
        "X11 keystroke unsupported for non-Latin-1 char U+{cp:04X} — \
         paste via clipboard+Ctrl+V or use ASCII"
    )))
}

pub(super) fn tap_key(
    conn: &RustConnection,
    root: u32,
    map: &HashMap<u32, (u8, bool)>,
    keysym: u32,
) -> Result<()> {
    let (keycode, need_shift) = map.get(&keysym).copied().ok_or_else(|| {
        PhenoError::Platform(format!(
            "no keycode for keysym 0x{keysym:x} on current X11 layout"
        ))
    })?;
    let (shift_kc, _) = if need_shift {
        map.get(&XK_SHIFT_L)
            .copied()
            .ok_or_else(|| PhenoError::Platform("Shift_L keycode missing from X11 keymap".into()))?
    } else {
        (0, false)
    };
    if need_shift {
        fake_input(conn, KEY_PRESS_EVENT, shift_kc, root, 0, 0)?;
    }
    fake_input(conn, KEY_PRESS_EVENT, keycode, root, 0, 0)?;
    fake_input(conn, KEY_RELEASE_EVENT, keycode, root, 0, 0)?;
    if need_shift {
        fake_input(conn, KEY_RELEASE_EVENT, shift_kc, root, 0, 0)?;
    }
    Ok(())
}

pub(super) fn chord_ctrl(
    conn: &RustConnection,
    root: u32,
    map: &HashMap<u32, (u8, bool)>,
    keysym: u32,
) -> Result<()> {
    let (ctrl_kc, _) = map
        .get(&XK_CONTROL_L)
        .copied()
        .ok_or_else(|| PhenoError::Platform("Control_L keycode missing from X11 keymap".into()))?;
    let (keycode, _) = map.get(&keysym).copied().ok_or_else(|| {
        PhenoError::Platform(format!(
            "no keycode for keysym 0x{keysym:x} on current X11 layout"
        ))
    })?;
    fake_input(conn, KEY_PRESS_EVENT, ctrl_kc, root, 0, 0)?;
    fake_input(conn, KEY_PRESS_EVENT, keycode, root, 0, 0)?;
    fake_input(conn, KEY_RELEASE_EVENT, keycode, root, 0, 0)?;
    fake_input(conn, KEY_RELEASE_EVENT, ctrl_kc, root, 0, 0)?;
    Ok(())
}

pub(super) fn inject_text(
    conn: &RustConnection,
    screen_num: usize,
    text: &str,
    delay_ms: u32,
) -> Result<()> {
    let root = root_window(conn, screen_num)?;
    let map = build_keysym_map(conn)?;
    for ch in text.chars() {
        let keysym = keysym_for_char(ch)?;
        tap_key(conn, root, &map, keysym)?;
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms as u64));
        }
    }
    Ok(())
}
