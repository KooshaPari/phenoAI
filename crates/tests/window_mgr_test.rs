//! Integration tests for the `port-window-mgr` port trait.
//!
//! These tests exercise the trait via a third-party mock that lives in
//! this test file only — they prove the trait can be implemented by
//! any adapter without taking a dependency on the host's concrete
//! adapter types.

use port_window_mgr::{Window, WindowId, WindowManager, WmError};

/// A mock window manager with a hand-seeded window list and a focus
/// recorder. It has no relationship to any real OS window manager
/// (X11 EWMH / NSWorkspace / Windows EnumWindows).
struct MockWindowManager {
    windows: std::sync::Mutex<Vec<Window>>,
    focus_calls: std::sync::Mutex<Vec<WindowId>>,
    fail_list: std::sync::Mutex<bool>,
}

impl MockWindowManager {
    fn new() -> Self {
        Self {
            windows: std::sync::Mutex::new(Vec::new()),
            focus_calls: std::sync::Mutex::new(Vec::new()),
            fail_list: std::sync::Mutex::new(false),
        }
    }

    fn seed(&self, windows: Vec<Window>) {
        *self.windows.lock().unwrap() = windows;
    }

    fn force_list_failure(&self) {
        *self.fail_list.lock().unwrap() = true;
    }
}

impl WindowManager for MockWindowManager {
    fn list(&self) -> Result<Vec<Window>, WmError> {
        if *self.fail_list.lock().unwrap() {
            return Err(WmError::ListFailed("mock-list-fail".into()));
        }
        Ok(self.windows.lock().unwrap().clone())
    }

    fn focus(&self, id: WindowId) -> Result<(), WmError> {
        let known = self.windows.lock().unwrap().iter().any(|w| w.id == id);
        if !known {
            return Err(WmError::WindowNotFound(id));
        }
        self.focus_calls.lock().unwrap().push(id);
        Ok(())
    }
}

/// Canonical "empty adapter" test from the L4 #61 spec: a fresh
/// window manager (no windows registered) MUST return an empty list,
/// not an error.
#[test]
fn window_manager_lists_empty() {
    let wm = MockWindowManager::new();
    let list = wm.list().expect("empty adapter must list without error");
    assert!(
        list.is_empty(),
        "expected empty list, got {} windows",
        list.len()
    );
}

/// `list` must return the seeded windows in the same order they were
/// registered (the application core relies on the order for
/// stable-by-index lookups in headless tests).
#[test]
fn window_manager_lists_seeded_windows() {
    let wm = MockWindowManager::new();
    wm.seed(vec![
        Window {
            id: WindowId(1),
            title: "Editor".into(),
            pid: 100,
            visible: true,
        },
        Window {
            id: WindowId(2),
            title: "Browser".into(),
            pid: 200,
            visible: true,
        },
        Window {
            id: WindowId(3),
            title: "Hidden".into(),
            pid: 300,
            visible: false,
        },
    ]);
    let list = wm.list().expect("seeded list must succeed");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].title, "Editor");
    assert_eq!(list[1].title, "Browser");
    assert!(!list[2].visible, "visibility flag must round-trip");
}

/// `focus` must record the call and return `Ok` for known ids; for
/// unknown ids it must return `WindowNotFound` (NOT silently no-op —
/// the L4 #61 spec explicitly forbids that to make UI bugs visible).
#[test]
fn window_manager_focus_known_and_unknown() {
    let wm = MockWindowManager::new();
    wm.seed(vec![Window {
        id: WindowId(7),
        title: "Target".into(),
        pid: 7,
        visible: true,
    }]);

    wm.focus(WindowId(7))
        .expect("focus on known id must succeed");
    let calls = wm.focus_calls.lock().unwrap().clone();
    assert_eq!(calls, vec![WindowId(7)], "focus call must be recorded");

    let err = wm
        .focus(WindowId(999))
        .expect_err("focus on unknown id must fail");
    assert_eq!(err.kind(), "window_not_found");
}

/// `list` failure must surface as a typed `WmError::ListFailed` (the
/// caller relies on this to decide whether to retry vs. fall back to
/// a different adapter).
#[test]
fn window_manager_list_failure_surfaces_typed_error() {
    let wm = MockWindowManager::new();
    wm.force_list_failure();
    let err = wm.list().expect_err("forced failure must surface");
    assert_eq!(err.kind(), "list_failed");
    assert!(
        err.to_string().contains("mock-list-fail"),
        "Display must include reason"
    );
}
