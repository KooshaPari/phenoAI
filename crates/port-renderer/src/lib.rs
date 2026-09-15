//! `port-renderer` — Renderer port trait for the PlayCua hex refactor (L4 #61).
//!
//! This crate declares the abstract boundary between the application core and
//! any concrete rendering backend. Adapters that implement [`Renderer`] live
//! outside this crate (e.g. a software pixel-buffer renderer, an OpenGL/Vulkan
//! adapter, a headless software renderer for tests, or a WebGPU backend);
//! the application core only ever holds `Box<dyn Renderer>` /
//! `Arc<dyn Renderer>` and dispatches through the trait.
//!
//! The trait is intentionally **object-safe** so the host can swap adapters
//! at runtime: no associated types, no generic methods, only `&self`
//! receivers, `Send + Sync` super-traits. This is the load-bearing invariant
//! that makes the composition root in `playcua-app` trivial to wire.
//!
//! # Example
//!
//! ```rust
//! use port_renderer::{Renderer, RenderOutput, RenderError, Frame, PixelFormat};
//!
//! struct NullRenderer;
//! impl Renderer for NullRenderer {
//!     fn render(&self, _frame: &Frame) -> Result<RenderOutput, RenderError> {
//!         Ok(RenderOutput { width: 0, height: 0, format: PixelFormat::Rgba8, draw_calls: 0 })
//!     }
//! }
//!
//! let r: Box<dyn Renderer> = Box::new(NullRenderer);
//! let out = r.render(&Frame { width: 1, height: 1, format: PixelFormat::Rgba8 })?;
//! assert_eq!(out.draw_calls, 0);
//! # Ok::<(), RenderError>(())
//! ```
//!
//! # Error model
//!
//! All fallible operations return [`Result<_, RenderError>`]. The error enum
//! is a `thiserror`-derived `Send + Sync + 'static` type with a stable
//! `kind(&self) -> &'static str` tag for log fields and metrics labels.

use thiserror::Error;

/// Pixel formats a [`Frame`] can carry. Kept deliberately small — concrete
/// adapters can convert to/from their native format on `render`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 32 bits per pixel, 8 bits per channel, R-G-B-A byte order.
    Rgba8,
    /// 32 bits per pixel, 8 bits per channel, B-G-R-A byte order.
    Bgra8,
    /// 24 bits per pixel, 8 bits per channel, R-G-B byte order.
    Rgb8,
    /// 8 bits per pixel, single grayscale channel.
    Gray8,
}

/// A frame handed to [`Renderer::render`].
///
/// `width` and `height` are in physical pixels. The `format` describes the
/// byte layout of the framebuffer; concrete adapters may translate to a
/// native GPU format on the fly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
}

/// What the renderer produced for a single `render` call.
///
/// `draw_calls` is surfaced so the application core (or a benchmark harness)
/// can assert that an adapter actually exercised the GPU path rather than
/// silently no-oping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub draw_calls: u32,
}

/// Errors a [`Renderer`] adapter can return.
#[derive(Debug, Error)]
pub enum RenderError {
    /// The adapter refused to render the supplied frame (e.g. zero-area
    /// frame, unsupported pixel format, dimension overflow).
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
    /// The backend's resource acquisition failed (e.g. surface lost, device
    /// removed, OOM, GPU driver crash).
    #[error("backend error: {0}")]
    Backend(String),
    /// The adapter does not support the requested frame's pixel format.
    /// Distinct from `InvalidFrame` so callers can fall back to a
    /// different adapter.
    #[error("unsupported format: {0:?}")]
    UnsupportedFormat(PixelFormat),
}

impl RenderError {
    /// Stable string tag for log fields and metrics labels.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidFrame(_) => "invalid_frame",
            Self::Backend(_) => "backend",
            Self::UnsupportedFormat(_) => "unsupported_format",
        }
    }
}

/// Renderer port — the abstract boundary for the rendering side of the
/// PlayCua hex refactor.
///
/// # Object safety
///
/// The trait is object-safe by construction: no associated types, no
/// generic methods, only `&self` receivers, `Send + Sync` super-traits.
/// This is the load-bearing invariant that makes
/// `Box<dyn Renderer>` storage in the composition root possible.
pub trait Renderer: Send + Sync {
    /// Render `frame` and return the [`RenderOutput`] describing what
    /// the adapter produced. Adapters MUST be safe to call from multiple
    /// threads concurrently (the `Sync` super-trait is the contract).
    fn render(&self, frame: &Frame) -> Result<RenderOutput, RenderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal in-test renderer that always succeeds with a
    /// deterministic output derived from the input frame's dimensions.
    struct IdentityRenderer;

    impl Renderer for IdentityRenderer {
        fn render(&self, frame: &Frame) -> Result<RenderOutput, RenderError> {
            Ok(RenderOutput {
                width: frame.width,
                height: frame.height,
                format: frame.format,
                draw_calls: 1,
            })
        }
    }

    /// `Renderer::render` must echo the frame's dimensions and format back
    /// in the `RenderOutput` (the round-trip contract that every adapter
    /// must honor).
    #[test]
    fn renderer_echoes_frame_dimensions() {
        let r = IdentityRenderer;
        let frame = Frame {
            width: 320,
            height: 240,
            format: PixelFormat::Rgba8,
        };
        let out = r.render(&frame).expect("identity render must succeed");
        assert_eq!(out.width, 320);
        assert_eq!(out.height, 240);
        assert_eq!(out.format, PixelFormat::Rgba8);
    }

    /// `Box<dyn Renderer>` storage is the load-bearing invariant for the
    /// composition root; this test fails to compile if the trait ever
    /// loses object safety.
    #[test]
    fn renderer_is_object_safe_box_dyn() {
        let r: Box<dyn Renderer> = Box::new(IdentityRenderer);
        let out = r
            .render(&Frame {
                width: 1,
                height: 1,
                format: PixelFormat::Rgba8,
            })
            .expect("boxed render must succeed");
        assert_eq!(out.draw_calls, 1);
    }

    /// `RenderError::kind()` must be a stable `&'static str` so callers
    /// can use it as a Prometheus label or log field.
    #[test]
    fn render_error_kind_is_stable_str() {
        let cases = [
            (
                RenderError::InvalidFrame("zero-area".into()).kind(),
                "invalid_frame",
            ),
            (RenderError::Backend("gpu lost".into()).kind(), "backend"),
            (
                RenderError::UnsupportedFormat(PixelFormat::Gray8).kind(),
                "unsupported_format",
            ),
        ];
        for (got, want) in cases {
            assert_eq!(got, want, "kind() tag drift: {got} != {want}");
        }
    }
}
