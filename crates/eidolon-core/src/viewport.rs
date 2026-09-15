use serde::{Deserialize, Serialize};

/// Viewport dimensions and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    /// Display width in pixels.
    pub width: u32,
    /// Display height in pixels.
    pub height: u32,
    /// Device pixel ratio (DPI scaling).
    pub dpr: f64,
    /// Orientation: "portrait" or "landscape".
    pub orientation: String,
}

impl Viewport {
    pub fn new(width: u32, height: u32, dpr: f64) -> Self {
        let orientation = if width > height {
            "landscape".to_string()
        } else {
            "portrait".to_string()
        };
        Self {
            width,
            height,
            dpr,
            orientation,
        }
    }

    /// Desktop standard: 1920x1080 @ 1.0 DPI.
    pub fn desktop_fhd() -> Self {
        Self::new(1920, 1080, 1.0)
    }

    /// Mobile standard: 1080x1920 @ 2.0 DPI (portrait).
    pub fn mobile_fhd() -> Self {
        Self::new(1080, 1920, 2.0)
    }

    /// Tablet standard: 2560x1440 @ 1.5 DPI.
    pub fn tablet_qhd() -> Self {
        Self::new(2560, 1440, 1.5)
    }
}
