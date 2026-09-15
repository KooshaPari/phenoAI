//! Integration tests for the `SerializationPort` hexagonal port.
//!
//! `SerializationPort` is the trait that drives project /
//! session save and load.  The crate ships a JSON-on-disk
//! adapter (`JsonFileSerializer`) and a recording mock
//! (`MockSerializationPort`).  These tests exercise both from a
//! separate test binary.

use std::path::PathBuf;

use kmobile_core::{JsonFileSerializer, KMobileError, MockSerializationPort, SerializationPort};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Sample {
    name: String,
    count: u32,
}

/// FR-KMOBILE-PORT-SERIAL-IT-000 — the JSON adapter round-trips
/// a value losslessly through a real file, and reports a stable
/// `format_id`.
#[tokio::test]
async fn serialization_port_json_roundtrip_real_file() {
    let tmp = TempDir::new().expect("tempdir");
    let path: PathBuf = tmp.path().join("sample.json");
    let ser = JsonFileSerializer::new();

    assert_eq!(ser.format_id(), "kmobile-json-v1");

    let original = Sample {
        name: "alpha".into(),
        count: 7,
    };
    ser.save(&original, &path).await.expect("save");

    // The file exists, is non-empty, and is valid JSON.
    let bytes = tokio::fs::read(&path).await.expect("read");
    assert!(!bytes.is_empty());
    let as_str = std::str::from_utf8(&bytes).expect("utf8");
    assert!(as_str.contains("\"name\""), "raw json: {as_str}");
    assert!(as_str.contains("\"alpha\""));

    let recovered: Sample = ser.load(&path).await.expect("load");
    assert_eq!(recovered, original);
}

/// FR-KMOBILE-PORT-SERIAL-IT-001 — the JSON adapter creates
/// missing parent directories on `save`, but a `load` against a
/// missing file surfaces `FileSystemError` (not a panic).
#[tokio::test]
async fn serialization_port_load_missing_file_errors() {
    let tmp = TempDir::new().expect("tempdir");
    let path: PathBuf = tmp.path().join("nested").join("does-not-exist.json");
    let ser = JsonFileSerializer::new();

    let err: Result<Sample, _> = ser.load(&path).await;
    assert!(matches!(err, Err(KMobileError::FileSystemError(_))));

    // save() should have created the parent dir.
    ser.save(
        &Sample {
            name: "x".into(),
            count: 1,
        },
        &path,
    )
    .await
    .expect("save creates parents");
    assert!(path.exists());
}

/// FR-KMOBILE-PORT-SERIAL-IT-002 — `MockSerializationPort`
/// records the latest saved bytes, replays a staged payload on
/// `load`, and counts every call. (The async `&self` `load`
/// surface borrows the staged bytes but does not consume them
/// or bump the counter; the mutating counterpart is `record_load`.)
#[tokio::test]
async fn serialization_port_mock_records_and_replays() {
    let mut mock = MockSerializationPort::default();

    // Load on a fresh mock with nothing staged → SerializationError.
    let err: Result<Sample, _> = mock.load(std::path::Path::new("/no/path")).await;
    assert!(matches!(err, Err(KMobileError::SerializationError(_))));
    // The async `&self` surface does not bump `load_count`.
    assert_eq!(mock.load_count(), 0);

    // Stage a payload and load it through the async surface.
    let staged = Sample {
        name: "beta".into(),
        count: 42,
    };
    let bytes = serde_json::to_vec_pretty(&staged).expect("encode");
    mock.stage_load_bytes(bytes);

    let recovered: Sample = mock
        .load(std::path::Path::new("/anything"))
        .await
        .expect("load");
    assert_eq!(recovered, staged);

    // The async `load` borrows the staged payload; calling it
    // again returns the same value.
    let recovered_again: Sample = mock
        .load(std::path::Path::new("/anything"))
        .await
        .expect("load again");
    assert_eq!(recovered_again, staged);

    // The mutating counterpart `record_load` consumes the staged
    // payload and bumps `load_count`.
    let consumed: Sample = mock.record_load().expect("record_load");
    assert_eq!(consumed, staged);
    assert_eq!(mock.load_count(), 1);

    // A subsequent `record_load` (nothing staged) errors.
    let err: Result<Sample, _> = mock.record_load();
    assert!(matches!(err, Err(KMobileError::SerializationError(_))));

    // `record_save` captures the latest bytes and bumps the count.
    mock.record_save(&staged).expect("record_save");
    assert_eq!(mock.save_count(), 1);
    let last = mock.last_saved_bytes().expect("last saved");
    let as_str = std::str::from_utf8(last).expect("utf8");
    assert!(as_str.contains("\"beta\""));
}

/// FR-KMOBILE-PORT-SERIAL-IT-003 — `JsonFileSerializer` is
/// `Send + Sync` and can be shared across awaits via `Arc`.
/// (The trait itself is not dyn-compatible because of the
/// generic `save` / `load` methods — that's by design; production
/// code stores the concrete adapter in an `Arc<JsonFileSerializer>`.)
#[tokio::test]
async fn serialization_port_is_shareable_via_arc() {
    use std::sync::Arc;

    let port: Arc<JsonFileSerializer> = Arc::new(JsonFileSerializer::new());

    let tmp = TempDir::new().expect("tempdir");
    let p1 = Arc::clone(&port);
    let p2 = Arc::clone(&port);
    let path1: PathBuf = tmp.path().join("a.json");
    let path2: PathBuf = tmp.path().join("b.json");

    let path1_for_load = path1.clone();
    let path2_for_load = path2.clone();
    let h1 = tokio::spawn(async move {
        p1.save(
            &Sample {
                name: "a".into(),
                count: 1,
            },
            &path1,
        )
        .await
    });
    let h2 = tokio::spawn(async move {
        p2.save(
            &Sample {
                name: "b".into(),
                count: 2,
            },
            &path2,
        )
        .await
    });

    h1.await.expect("join 1").expect("save 1");
    h2.await.expect("join 2").expect("save 2");

    // Both files exist and round-trip correctly.
    let a: Sample = port.load(&path1_for_load).await.expect("load a");
    let b: Sample = port.load(&path2_for_load).await.expect("load b");
    assert_eq!(a.name, "a");
    assert_eq!(b.count, 2);
}
