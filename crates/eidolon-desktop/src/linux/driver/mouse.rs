//! X11 mouse/pointer injection helpers.

use eidolon_core::Result;
use x11rb::protocol::xproto::MOTION_NOTIFY_EVENT;
use x11rb::rust_connection::RustConnection;

use super::{fake_input, root_window, screen_size};

pub(super) fn move_absolute(
    conn: &RustConnection,
    screen_num: usize,
    x: i32,
    y: i32,
) -> Result<()> {
    let root = root_window(conn, screen_num)?;
    let (sw, sh) = screen_size(conn, screen_num)?;
    let x = x.clamp(0, i32::from(sw.saturating_sub(1))) as i16;
    let y = y.clamp(0, i32::from(sh.saturating_sub(1))) as i16;
    fake_input(conn, MOTION_NOTIFY_EVENT, 0, root, x, y)
}

pub(super) fn button_detail(button: &Option<String>) -> u8 {
    match button.as_deref() {
        Some("middle") => 2,
        Some("right") => 3,
        _ => 1,
    }
}
