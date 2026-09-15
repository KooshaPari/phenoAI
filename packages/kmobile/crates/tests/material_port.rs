//! Integration tests for the `MaterialPort` hexagonal port.
//!
//! `MaterialPort` is the trait that manages build assets
//! (icons, splashes, signing keys, push certificates).  The crate
//! already ships two in-memory adapters and a recording mock —
//! these tests exercise them through the public async trait
//! surface from a separate test binary, which catches trait-
//! breaking changes that in-crate unit tests would miss.

use std::path::PathBuf;

use kmobile_core::{
    AssetInfo, AssetKind, InMemoryMaterialPort, MaterialPort, MockMaterialCall, MockMaterialPort,
    MutableInMemoryMaterialPort,
};

/// FR-KMOBILE-PORT-MATERIAL-IT-000 — `AssetInfo` and `AssetKind`
/// round-trip through JSON so assets can be persisted in
/// `.kmobile/material.json` and shipped over MCP.
#[tokio::test]
async fn material_port_asset_info_serde_roundtrip() {
    let original = AssetInfo {
        id: "asset-7".into(),
        kind: AssetKind::SigningKey,
        path: PathBuf::from("/opt/kmobile/keys/dist.p12"),
        size_bytes: 4096,
    };

    let json = serde_json::to_string(&original).expect("serialize");
    let recovered: AssetInfo = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(recovered, original);

    // Each AssetKind variant serializes with snake_case.
    for (kind, expected) in [
        (AssetKind::Icon, "\"icon\""),
        (AssetKind::Splash, "\"splash\""),
        (AssetKind::SigningKey, "\"signing_key\""),
        (AssetKind::PushCertificate, "\"push_certificate\""),
        (AssetKind::Other, "\"other\""),
    ] {
        let encoded = serde_json::to_string(&kind).expect("encode");
        assert_eq!(encoded, expected, "AssetKind::{kind:?} mismatch");
    }
}

/// FR-KMOBILE-PORT-MATERIAL-IT-001 — `InMemoryMaterialPort`
/// rejects empty paths with `InvalidInput`, surfaces the
/// `AssetInfo` from `add_asset`, and never persists (the
/// async `&self` surface is a snapshot view).
#[tokio::test]
async fn material_port_in_memory_rejects_empty_path() {
    let port = InMemoryMaterialPort::new();

    // Empty path → InvalidInput.
    let err = port
        .add_asset(None, AssetKind::Icon, PathBuf::new())
        .await
        .expect_err("empty path must error");
    assert!(matches!(err, kmobile_core::KMobileError::InvalidInput(_)));

    // Non-existent path → FileSystemError (we try to stat it).
    let err = port
        .add_asset(
            None,
            AssetKind::Icon,
            PathBuf::from("/this/path/does/not/exist.png"),
        )
        .await
        .expect_err("missing file must error");
    assert!(matches!(
        err,
        kmobile_core::KMobileError::FileSystemError(_)
    ));

    // list_assets on a fresh port is empty.
    let assets = port.list_assets(None).await.expect("list");
    assert!(assets.is_empty());
}

/// FR-KMOBILE-PORT-MATERIAL-IT-002 — `MutableInMemoryMaterialPort`
/// retains assets across reads (mut helper) and the async
/// `&self` surface returns the canonical id assigned at insert
/// time.
#[tokio::test]
async fn material_port_mutable_round_trip() {
    let mut port = MutableInMemoryMaterialPort::default();

    let icon = port
        .add_asset_mut(AssetKind::Icon, PathBuf::from("/tmp/icon.png"))
        .expect("add icon");
    let splash = port
        .add_asset_mut(AssetKind::Splash, PathBuf::from("/tmp/splash.png"))
        .expect("add splash");

    // Async read returns the same ids and kinds in insertion order.
    let assets = port.list_assets(None).await.expect("list");
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0].id, icon.id);
    assert_eq!(assets[0].kind, AssetKind::Icon);
    assert_eq!(assets[1].id, splash.id);
    assert_eq!(assets[1].kind, AssetKind::Splash);

    // Lookup by id works.
    let found = port
        .get_asset(None, &icon.id)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(found.kind, AssetKind::Icon);

    // Remove by id works and is idempotent.
    assert!(port.remove_asset_mut(&splash.id));
    assert!(!port.remove_asset_mut(&splash.id));
    assert_eq!(port.snapshot().len(), 1);
}

/// FR-KMOBILE-PORT-MATERIAL-IT-003 — `MockMaterialPort` records
/// the full call sequence so domain tests can assert "the domain
/// added an icon, then a splash, then queried for the icon".
#[tokio::test]
async fn material_port_mock_records_call_sequence() {
    let mut mock = MockMaterialPort::default();

    let icon = mock.record_add(AssetKind::Icon, PathBuf::from("/tmp/icon.png"));
    mock.record_get(&icon.id);
    let splash = mock.record_add(AssetKind::Splash, PathBuf::from("/tmp/splash.png"));
    mock.record_remove(&splash.id);
    mock.reset_calls();

    // After remove + reset_calls: storage is [icon] (splash was
    // removed) and the call log is empty.
    assert_eq!(mock.snapshot().len(), 1);
    assert!(mock.calls().is_empty());

    mock.record_add(AssetKind::PushCertificate, PathBuf::from("/tmp/cert.pem"));

    // After reset_calls, only the post-reset call remains in the
    // log; storage now holds [icon, push_certificate].
    assert_eq!(mock.calls().len(), 1);
    assert!(matches!(
        mock.calls()[0],
        MockMaterialCall::Add {
            kind: AssetKind::PushCertificate,
            ..
        }
    ));

    let snap = mock.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].kind, AssetKind::Icon);
    assert_eq!(snap[1].kind, AssetKind::PushCertificate);
}

/// FR-KMOBILE-PORT-MATERIAL-IT-004 — the trait is
/// `Send + Sync` and can be stored as `Arc<dyn MaterialPort>`,
/// the shape used by the CLI / API / MCP servers.
#[tokio::test]
async fn material_port_is_dyn_compatible() {
    use std::sync::Arc;

    let port: Arc<dyn MaterialPort> = Arc::new(InMemoryMaterialPort::new());

    let p1 = Arc::clone(&port);
    let p2 = Arc::clone(&port);
    let h1 = tokio::spawn(async move { p1.list_assets(None).await });
    let h2 = tokio::spawn(async move { p2.list_assets(None).await });

    let r1 = h1.await.expect("join 1").expect("list 1");
    let r2 = h2.await.expect("join 2").expect("list 2");

    assert!(r1.is_empty());
    assert!(r2.is_empty());
}
