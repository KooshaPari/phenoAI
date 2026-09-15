//! Integration tests for the `port-renderer` port trait.
//!
//! These tests exercise the trait via a third-party mock that lives in
//! this test file only — they prove the trait can be implemented by
//! any adapter without taking a dependency on the host's concrete
//! adapter types.

use port_renderer::{Frame, PixelFormat, RenderError, RenderOutput, Renderer};

/// A mock renderer that returns whatever was seeded in
/// `next_output`; if `None`, it returns a deterministic output derived
/// from the input frame.
struct MockRenderer {
    next_output: std::sync::Mutex<Option<RenderOutput>>,
    fail_with: std::sync::Mutex<Option<RenderError>>,
}

impl MockRenderer {
    fn new() -> Self {
        Self {
            next_output: std::sync::Mutex::new(None),
            fail_with: std::sync::Mutex::new(None),
        }
    }

    #[allow(dead_code)]
    fn seed_output(&self, out: RenderOutput) {
        *self.next_output.lock().unwrap() = Some(out);
    }

    fn seed_failure(&self, err: RenderError) {
        *self.fail_with.lock().unwrap() = Some(err);
    }
}

impl Renderer for MockRenderer {
    fn render(&self, frame: &Frame) -> Result<RenderOutput, RenderError> {
        if let Some(err) = self.fail_with.lock().unwrap().take() {
            return Err(err);
        }
        if let Some(out) = self.next_output.lock().unwrap().take() {
            return Ok(out);
        }
        Ok(RenderOutput {
            width: frame.width,
            height: frame.height,
            format: frame.format,
            draw_calls: 0,
        })
    }
}

/// The canonical "happy path" test from the L4 #61 spec: render a known
/// frame and verify the output dimensions + format round-trip.
#[test]
fn renderer_renders_known_frame() {
    let r = MockRenderer::new();
    let frame = Frame {
        width: 1920,
        height: 1080,
        format: PixelFormat::Rgba8,
    };
    let out = r.render(&frame).expect("known-good frame must render");
    assert_eq!(out.width, 1920);
    assert_eq!(out.height, 1080);
    assert_eq!(out.format, PixelFormat::Rgba8);
}

/// `Renderer::render` must propagate `RenderError::UnsupportedFormat`
/// unchanged so the caller can decide whether to fall back to a
/// different adapter.
#[test]
fn renderer_propagates_unsupported_format_error() {
    let r = MockRenderer::new();
    r.seed_failure(RenderError::UnsupportedFormat(PixelFormat::Gray8));
    let frame = Frame {
        width: 8,
        height: 8,
        format: PixelFormat::Gray8,
    };
    let err = r.render(&frame).expect_err("seeded failure must surface");
    assert_eq!(err.kind(), "unsupported_format");
}

/// Trait objects must be sendable across threads — proves the
/// `Send + Sync` super-trait is wired correctly.
#[test]
fn renderer_trait_object_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Box<dyn Renderer>>();

    let r: Box<dyn Renderer> = Box::new(MockRenderer::new());
    let join = std::thread::spawn(move || {
        r.render(&Frame {
            width: 1,
            height: 1,
            format: PixelFormat::Rgba8,
        })
        .map(|out| out.draw_calls)
    });
    let draw_calls = join
        .join()
        .expect("worker thread must not panic")
        .expect("render ok");
    assert_eq!(draw_calls, 0);
}

/// `RenderError` must implement `std::error::Error + Send + Sync + 'static`
/// (the standard error-trait battery) so the host can box it as
/// `Box<dyn std::error::Error>` and `?`-propagate from `main`.
#[test]
fn render_error_is_std_error_send_sync_static() {
    fn assert_error_bound<T>()
    where
        T: std::error::Error + Send + Sync + 'static,
    {
    }
    assert_error_bound::<RenderError>();

    let err: Box<dyn std::error::Error + Send + Sync + 'static> =
        Box::new(RenderError::Backend("driver crashed".into()));
    let msg = err.to_string();
    assert!(
        msg.contains("backend"),
        "Display should mention variant: {msg}"
    );
    assert!(
        msg.contains("driver crashed"),
        "Display should include reason: {msg}"
    );
}
