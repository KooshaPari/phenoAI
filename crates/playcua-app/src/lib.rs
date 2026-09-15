//! `playcua-app` — composition root for the PlayCua hex refactor (L4 #61).
//!
//! This crate is intentionally tiny. Its only job is to **wire** the three
//! port traits together via trait objects so the rest of the application
//! core can be written against abstract ports and stay ignorant of which
//! concrete adapter (X11 / macOS / Windows / enigo / WebDriver / headless
//! mock / ...) is actually plugged in.
//!
//! The shape of the composition root is the canonical hex-refactor
//! pattern:
//!
//! 1. The application core depends only on the three port traits
//!    (`port_renderer::Renderer`, `port_window_mgr::WindowManager`,
//!    `port_input::InputSource`).
//! 2. Adapters live in separate crates (or, in this scaffolding, in
//!    in-tree mock implementations) and are injected at startup via
//!    `Box<dyn Trait>`.
//! 3. The `App` struct holds three boxed trait objects; nothing about
//!    the host's runtime concerns (threading, async runtime, OS APIs)
//!    leaks into the application code.
//!
//! # Example
//!
//! ```rust
//! use playcua_app::{build_app, in_tree_mocks, App};
//! use port_renderer::{Frame, PixelFormat};
//! use port_window_mgr::WindowId;
//!
//! let app: App = build_app(in_tree_mocks::renderer(), in_tree_mocks::window_mgr(), in_tree_mocks::input_source());
//! let _out = app.render_frame(&Frame { width: 1, height: 1, format: PixelFormat::Rgba8 });
//! let _list = app.list_windows();
//! let _evt  = app.next_input();
//! let _ = app.focus_window(WindowId(0));
//! ```

use port_input::InputSource;
use port_renderer::Renderer;
use port_window_mgr::WindowManager;

/// The composition root — the only place in the application that knows
/// about the concrete adapter types. Hold the trait objects in
/// heap-allocated boxes so the struct is `Send + Sync` and the adapter
/// types can be swapped per-build (or per-test) without recompiling
/// the application core.
pub struct App {
    renderer: Box<dyn Renderer>,
    window_mgr: Box<dyn WindowManager>,
    input: Box<dyn InputSource>,
}

impl App {
    /// Construct a new `App` from three boxed port-trait implementations.
    /// This is the **only** function that touches concrete adapter types
    /// — every other call site in the application takes `&App` and
    /// dispatches through the trait objects.
    pub fn new(
        renderer: Box<dyn Renderer>,
        window_mgr: Box<dyn WindowManager>,
        input: Box<dyn InputSource>,
    ) -> Self {
        Self {
            renderer,
            window_mgr,
            input,
        }
    }

    /// Render a single frame through the configured renderer.
    pub fn render_frame(
        &self,
        frame: &port_renderer::Frame,
    ) -> Result<port_renderer::RenderOutput, port_renderer::RenderError> {
        self.renderer.render(frame)
    }

    /// Enumerate the current top-level windows through the configured
    /// window manager.
    pub fn list_windows(&self) -> Result<Vec<port_window_mgr::Window>, port_window_mgr::WmError> {
        self.window_mgr.list()
    }

    /// Bring a window to the foreground through the configured window
    /// manager.
    pub fn focus_window(
        &self,
        id: port_window_mgr::WindowId,
    ) -> Result<(), port_window_mgr::WmError> {
        self.window_mgr.focus(id)
    }

    /// Block until the next input event from the configured source.
    pub fn next_input(&self) -> Result<port_input::InputEvent, port_input::InputError> {
        self.input.next_event()
    }
}

/// Build a new `App` from three boxed port-trait implementations. This
/// is a thin convenience over [`App::new`] so the `main` binary and
/// the integration tests share a single entry point.
pub fn build_app(
    renderer: Box<dyn Renderer>,
    window_mgr: Box<dyn WindowManager>,
    input: Box<dyn InputSource>,
) -> App {
    App::new(renderer, window_mgr, input)
}

/// In-tree mock adapters for the composition root. These exist so the
/// scaffolding has a runnable default (useful for `cargo run -p
/// playcua-app`, for the integration tests, and for the doctest above).
/// Production builds would replace these with the real platform
/// adapters (`x11_window_capturer`, `nsworkspace_window_mgr`,
/// `enigo_input_source`, ...).
pub mod in_tree_mocks {
    use super::*;
    use port_input::{InputError, InputEvent, Key, KeyAction};
    use port_renderer::{Frame, RenderError, RenderOutput};
    use port_window_mgr::{Window, WindowId, WmError};

    /// A renderer that echoes the frame's dimensions back. A no-op
    /// default that lets `cargo run -p playcua-app` succeed in a
    /// headless CI environment.
    pub struct EchoRenderer;

    impl Renderer for EchoRenderer {
        fn render(&self, frame: &Frame) -> Result<RenderOutput, RenderError> {
            Ok(RenderOutput {
                width: frame.width,
                height: frame.height,
                format: frame.format,
                draw_calls: 1,
            })
        }
    }

    /// A window manager that always reports an empty list. The default
    /// for headless / CI builds.
    pub struct EmptyWindowMgr;

    impl WindowManager for EmptyWindowMgr {
        fn list(&self) -> Result<Vec<Window>, WmError> {
            Ok(Vec::new())
        }
        fn focus(&self, _id: WindowId) -> Result<(), WmError> {
            Ok(())
        }
    }

    /// An input source that always emits a single `'h'` press event.
    /// The default for headless / CI builds.
    pub struct HelloInputSource;

    impl InputSource for HelloInputSource {
        fn next_event(&self) -> Result<InputEvent, InputError> {
            Ok(InputEvent::Key {
                key: Key("h".into()),
                action: KeyAction::Press,
            })
        }
    }

    /// `Box<dyn Renderer>` for the echo renderer.
    pub fn renderer() -> Box<dyn Renderer> {
        Box::new(EchoRenderer)
    }

    /// `Box<dyn WindowManager>` for the empty window manager.
    pub fn window_mgr() -> Box<dyn WindowManager> {
        Box::new(EmptyWindowMgr)
    }

    /// `Box<dyn InputSource>` for the hello input source.
    pub fn input_source() -> Box<dyn InputSource> {
        Box::new(HelloInputSource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use port_input::{InputError, InputEvent, Key, KeyAction};
    use port_renderer::{Frame, PixelFormat, RenderError, RenderOutput};
    use port_window_mgr::{Window, WindowId, WmError};

    /// A renderer that fails with a seeded `RenderError`. Used to
    /// prove the composition root propagates adapter errors verbatim
    /// (no re-wrap, no swallow).
    struct FailRenderer;
    impl Renderer for FailRenderer {
        fn render(&self, _frame: &Frame) -> Result<RenderOutput, RenderError> {
            Err(RenderError::Backend("driver crashed".into()))
        }
    }

    /// A window manager that returns a single known window. Used to
    /// prove the composition root round-trips `Vec<Window>` and
    /// `WindowId` through the trait boundary.
    struct OneWindowMgr;
    impl WindowManager for OneWindowMgr {
        fn list(&self) -> Result<Vec<Window>, WmError> {
            Ok(vec![Window {
                id: WindowId(11),
                title: "Solo".into(),
                pid: 1,
                visible: true,
            }])
        }
        fn focus(&self, _id: WindowId) -> Result<(), WmError> {
            Ok(())
        }
    }

    /// An input source that emits two events then signals
    /// `TransportClosed`. Used to prove the composition root handles
    /// both the success and the end-of-stream path.
    struct TwoThenClosed {
        remaining: std::sync::Mutex<u32>,
    }
    impl InputSource for TwoThenClosed {
        fn next_event(&self) -> Result<InputEvent, InputError> {
            let mut r = self.remaining.lock().unwrap();
            if *r == 0 {
                return Err(InputError::TransportClosed("queue empty".into()));
            }
            *r -= 1;
            Ok(InputEvent::Key {
                key: Key("x".into()),
                action: KeyAction::Release,
            })
        }
    }

    /// The composition root must wire all three trait objects and
    /// dispatch through them — proves the `App::new` constructor and
    /// every helper method (`render_frame`, `list_windows`,
    /// `focus_window`, `next_input`) round-trip without going through
    /// any concrete adapter type.
    #[test]
    fn composition_root_wires_all_three_ports() {
        let app = build_app(
            in_tree_mocks::renderer(),
            in_tree_mocks::window_mgr(),
            in_tree_mocks::input_source(),
        );

        let out = app
            .render_frame(&Frame {
                width: 16,
                height: 9,
                format: PixelFormat::Rgba8,
            })
            .expect("echo render must succeed");
        assert_eq!(out.width, 16);
        assert_eq!(out.height, 9);
        assert_eq!(out.draw_calls, 1);

        let list = app.list_windows().expect("empty wm must list");
        assert!(list.is_empty());

        app.focus_window(WindowId(0)).expect("empty wm must focus");

        let evt = app.next_input().expect("hello source must emit");
        assert_eq!(
            evt,
            InputEvent::Key {
                key: Key("h".into()),
                action: KeyAction::Press
            }
        );
    }

    /// Adapter errors must propagate verbatim through the composition
    /// root — the host must be able to distinguish `RenderError` from
    /// `WmError` from `InputError` based on the concrete type, not on
    /// a re-wrapped string.
    #[test]
    fn composition_root_propagates_adapter_errors_verbatim() {
        let app = build_app(
            Box::new(FailRenderer),
            Box::new(OneWindowMgr),
            Box::new(TwoThenClosed {
                remaining: std::sync::Mutex::new(0),
            }),
        );

        let err = app
            .render_frame(&Frame {
                width: 1,
                height: 1,
                format: PixelFormat::Rgba8,
            })
            .expect_err("fail-renderer must error");
        assert_eq!(err.kind(), "backend");

        let list = app.list_windows().expect("one-window wm must list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, WindowId(11));

        let err = app
            .next_input()
            .expect_err("empty two-then-closed must error");
        assert_eq!(err.kind(), "transport_closed");
    }

    /// `App` must be `Send + Sync` so a multi-threaded host (worker
    /// pool, TUI thread, IPC thread) can hold a shared reference.
    #[test]
    fn app_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<App>();
    }
}
