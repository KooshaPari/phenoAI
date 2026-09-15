//! Renderer port — abstract boundary for rendering operations.
//!
//! Provides an async trait [`Renderer`] with an in-memory test adapter
//! and a wire adapter for production.

use crate::domain::render::{RenderFrame, RenderOutput, RendererError};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Renderer port — the abstract boundary for the rendering side of the
/// PlayCua hex refactor.
#[async_trait]
pub trait Renderer: Send + Sync {
    /// Render a frame and return the output describing what the adapter
    /// produced.
    async fn render(&self, frame: &RenderFrame) -> Result<RenderOutput, RendererError>;
    /// Clear any internal state / buffers.
    async fn clear(&self) -> Result<(), RendererError>;
}

/// In-memory adapter for testing — records every rendered frame in a
/// shared buffer so tests can assert on the round-trip.
pub struct InMemoryRendererAdapter {
    frames: Arc<Mutex<Vec<RenderFrame>>>,
}

impl InMemoryRendererAdapter {
    pub fn new() -> Self {
        Self {
            frames: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return a snapshot of all frames rendered so far.
    pub async fn recorded_frames(&self) -> Vec<RenderFrame> {
        self.frames.lock().await.clone()
    }
}

impl Default for InMemoryRendererAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Renderer for InMemoryRendererAdapter {
    async fn render(&self, frame: &RenderFrame) -> Result<RenderOutput, RendererError> {
        self.frames.lock().await.push(frame.clone());
        Ok(RenderOutput {
            width: frame.width,
            height: frame.height,
            draw_calls: 1,
        })
    }

    async fn clear(&self) -> Result<(), RendererError> {
        self.frames.lock().await.clear();
        Ok(())
    }
}

/// Wire adapter for production — delegates to the real rendering backend.
///
/// Currently a stub that compiles and returns a minimal output; the real
/// backend integration is a follow-up task.
pub struct WireRendererAdapter;

impl WireRendererAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WireRendererAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Renderer for WireRendererAdapter {
    async fn render(&self, frame: &RenderFrame) -> Result<RenderOutput, RendererError> {
        Ok(RenderOutput {
            width: frame.width,
            height: frame.height,
            draw_calls: 0,
        })
    }

    async fn clear(&self) -> Result<(), RendererError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The in-memory adapter must echo frame dimensions back in the output.
    #[tokio::test]
    async fn in_memory_renderer_echoes_dimensions() {
        let adapter = InMemoryRendererAdapter::new();
        let frame = RenderFrame {
            width: 320,
            height: 240,
            data: vec![0u8; 320 * 240 * 4],
        };
        let out = adapter.render(&frame).await.expect("render must succeed");
        assert_eq!(out.width, 320);
        assert_eq!(out.height, 240);
        assert_eq!(out.draw_calls, 1);
    }

    /// The in-memory adapter must record frames so tests can inspect them.
    #[tokio::test]
    async fn in_memory_renderer_records_frames() {
        let adapter = InMemoryRendererAdapter::new();
        let frame = RenderFrame {
            width: 640,
            height: 480,
            data: vec![1u8; 640 * 480 * 4],
        };
        adapter.render(&frame).await.unwrap();
        let recorded = adapter.recorded_frames().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], frame);
    }

    /// Clear must empty the recorded frame buffer.
    #[tokio::test]
    async fn in_memory_renderer_clear_empties_buffer() {
        let adapter = InMemoryRendererAdapter::new();
        let frame = RenderFrame {
            width: 10,
            height: 10,
            data: vec![2u8; 10 * 10 * 4],
        };
        adapter.render(&frame).await.unwrap();
        adapter.clear().await.unwrap();
        let recorded = adapter.recorded_frames().await;
        assert!(recorded.is_empty());
    }

    /// The wire adapter must compile and return a minimal output.
    #[tokio::test]
    async fn wire_renderer_compiles_and_returns_output() {
        let adapter = WireRendererAdapter::new();
        let frame = RenderFrame {
            width: 128,
            height: 128,
            data: vec![3u8; 128 * 128 * 4],
        };
        let out = adapter
            .render(&frame)
            .await
            .expect("wire render must succeed");
        assert_eq!(out.width, 128);
        assert_eq!(out.height, 128);
    }

    /// `Box<dyn Renderer>` storage must compile (object safety).
    #[tokio::test]
    async fn renderer_is_object_safe() {
        let r: Box<dyn Renderer> = Box::new(InMemoryRendererAdapter::new());
        let frame = RenderFrame {
            width: 1,
            height: 1,
            data: vec![0u8; 4],
        };
        let out = r.render(&frame).await.expect("boxed render must succeed");
        assert_eq!(out.draw_calls, 1);
    }
}
