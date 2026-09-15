//! Unit tests for the NativeAnalysisAdapter (AnalysisPort implementation).
//!
//! These are integration-style tests under native/tests/ that compile against
//! the playcua-native crate in library mode. All test helpers are defined
//! inline — no external test-helper crates are required.

use playcua_native::adapters::analysis_adapter::NativeAnalysisAdapter;
use playcua_native::domain::analysis::AnalysisError;
use playcua_native::ports::AnalysisPort;

// ---------------------------------------------------------------------------
// Helper: encode a solid-color RGBA image as PNG bytes.
// ---------------------------------------------------------------------------

fn solid_png(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|_| [r, g, b, 255u8])
        .collect();
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(width, height, pixels).expect("ImageBuffer from_raw failed");
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut buf),
            image::ImageFormat::Png,
        )
        .expect("PNG encode failed");
    buf
}

// ---------------------------------------------------------------------------
// diff tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn diff_identical_frames_returns_zero_change_ratio() {
    let adapter = NativeAnalysisAdapter::new();
    let frame = solid_png(64, 64, 128, 64, 32);

    let result = adapter.diff(&frame, &frame, 0.02).await.unwrap();
    assert_eq!(
        result.change_ratio, 0.0,
        "identical frames must have change_ratio == 0.0"
    );
    assert!(!result.changed, "identical frames must not be marked as changed");
}

#[tokio::test]
async fn diff_all_black_vs_all_white_returns_one_change_ratio() {
    let adapter = NativeAnalysisAdapter::new();
    let black = solid_png(32, 32, 0, 0, 0);
    let white = solid_png(32, 32, 255, 255, 255);

    let result = adapter.diff(&black, &white, 0.02).await.unwrap();
    assert!(
        (result.change_ratio - 1.0).abs() < 1e-9,
        "all-black vs all-white must have change_ratio == 1.0, got {}",
        result.change_ratio
    );
    assert!(
        result.changed,
        "all-black vs all-white must be marked as changed"
    );
}

#[tokio::test]
async fn diff_change_ratio_is_always_in_unit_interval() {
    let adapter = NativeAnalysisAdapter::new();

    let test_cases: &[(u8, u8, u8, u8, u8, u8)] = &[
        (0, 0, 0, 255, 255, 255),
        (255, 0, 0, 0, 255, 0),
        (100, 100, 100, 101, 101, 101),
        (0, 128, 255, 0, 128, 255),
    ];

    for &(r1, g1, b1, r2, g2, b2) in test_cases {
        let a = solid_png(16, 16, r1, g1, b1);
        let b = solid_png(16, 16, r2, g2, b2);
        let result = adapter.diff(&a, &b, 0.0).await.unwrap();
        assert!(
            result.change_ratio >= 0.0 && result.change_ratio <= 1.0,
            "change_ratio {} is outside [0.0, 1.0] for ({},{},{}) vs ({},{},{})",
            result.change_ratio,
            r1,
            g1,
            b1,
            r2,
            g2,
            b2
        );
    }
}

#[tokio::test]
async fn diff_dimension_mismatch_returns_error() {
    let adapter = NativeAnalysisAdapter::new();
    let small = solid_png(16, 16, 0, 0, 0);
    let large = solid_png(32, 32, 0, 0, 0);

    let err = adapter.diff(&small, &large, 0.02).await.unwrap_err();
    assert!(
        matches!(err, AnalysisError::DimensionMismatch(16, 16, 32, 32)),
        "Expected DimensionMismatch(16,16,32,32), got: {err}"
    );
}

// ---------------------------------------------------------------------------
// hash tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hash_same_data_returns_same_string() {
    let adapter = NativeAnalysisAdapter::new();
    let frame = solid_png(32, 32, 42, 100, 200);

    let h1 = adapter.hash(&frame).await.unwrap();
    let h2 = adapter.hash(&frame).await.unwrap();
    assert_eq!(h1.hash, h2.hash, "same data must produce same hash");
}

#[tokio::test]
async fn hash_different_data_returns_different_strings() {
    let adapter = NativeAnalysisAdapter::new();
    let frame_a = solid_png(32, 32, 0, 0, 0);
    let frame_b = solid_png(32, 32, 255, 255, 255);

    let ha = adapter.hash(&frame_a).await.unwrap();
    let hb = adapter.hash(&frame_b).await.unwrap();
    assert_ne!(
        ha.hash, hb.hash,
        "different data must produce different hashes"
    );
}

#[tokio::test]
async fn hash_result_is_nonempty_hex_string() {
    let adapter = NativeAnalysisAdapter::new();
    let frame = solid_png(8, 8, 1, 2, 3);

    let result = adapter.hash(&frame).await.unwrap();
    assert!(!result.hash.is_empty(), "hash must not be empty");
    assert!(
        result.hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hash must be a valid hex string, got: {}",
        result.hash
    );
}
