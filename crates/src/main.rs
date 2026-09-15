//! `playcua-app` — PlayCua hex-refactor composition root binary.
//!
//! The binary is intentionally minimal: it constructs the composition
//! root with the in-tree mock adapters (see
//! [`playcua_app::in_tree_mocks`]) and runs a small smoke loop that
//! exercises every port exactly once.
//!
//! Production builds would replace the in-tree mocks with the real
//! platform adapters (X11 / macOS / Windows / WebDriver / enigo / ...).
//! The composition root pattern in `lib.rs` is designed to make that
//! swap a one-line change at the call sites of `playcua_app::build_app`.

use playcua_app::{build_app, in_tree_mocks};
use port_input::InputError;
use port_renderer::{Frame, PixelFormat};
use port_window_mgr::WindowId;

fn main() {
    // 1. Construct the composition root from three boxed port-trait
    //    implementations. This is the **only** place in the
    //    application that knows about concrete adapter types.
    let app = build_app(
        in_tree_mocks::renderer(),
        in_tree_mocks::window_mgr(),
        in_tree_mocks::input_source(),
    );

    // 2. Exercise every port exactly once — proves the wiring is
    //    live end-to-end and gives `cargo run -p playcua-app` a
    //    visible success signal in headless / CI environments.
    match app.render_frame(&Frame {
        width: 4,
        height: 4,
        format: PixelFormat::Rgba8,
    }) {
        Ok(out) => println!(
            "renderer: {}x{} {:?} draw_calls={}",
            out.width, out.height, out.format, out.draw_calls
        ),
        Err(e) => {
            eprintln!("renderer error: {e}");
            std::process::exit(1);
        }
    }

    match app.list_windows() {
        Ok(list) => println!("window_mgr: {} window(s)", list.len()),
        Err(e) => {
            eprintln!("window_mgr error: {e}");
            std::process::exit(1);
        }
    }

    if let Err(e) = app.focus_window(WindowId(0)) {
        eprintln!("focus_window error: {e}");
        std::process::exit(1);
    }
    println!("window_mgr: focus ok");

    // 3. For the input source we just print the first event and stop —
    //    blocking on a real OS event tap would hang the binary in a
    //    headless environment.
    match app.next_input() {
        Ok(evt) => println!("input_source: {evt:?}"),
        Err(InputError::TransportClosed(msg)) => {
            // Expected for a finite mock queue — exit cleanly with 0
            // so CI / smoke runs see success.
            println!("input_source: end-of-stream ({msg})");
        }
        Err(e) => {
            eprintln!("input_source error: {e}");
            std::process::exit(1);
        }
    }
}
