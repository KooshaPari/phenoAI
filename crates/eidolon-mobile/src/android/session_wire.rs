//! Wire-dialect translator for UIA2 instrumentation vs Appium/W3C servers.
//!
//! Direct UiAutomator2 (`:6790`) accepts Appium-driver-internal bodies
//! (`strategy` / `selector` / `multiple`). A full Appium server (`:4723` /
//! `EIDOLON_APPIUM_URL`) expects W3C WebDriver locator bodies (`using` /
//! `value`) and Appium-prefixed session capabilities before it proxies to
//! UIA2. Callers talking to Appium must use [`SessionWireMode::AppiumW3c`].

use serde_json::{json, Value};

/// HTTP body dialect for session verbs (`create_session`, `find_element(s)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionWireMode {
    /// Direct UiAutomator2 instrumentation server (`:6790`).
    #[default]
    Uia2Server,
    /// Full Appium server (`:4723` / `EIDOLON_APPIUM_URL`) — W3C + Appium caps.
    AppiumW3c,
}

impl SessionWireMode {
    /// Build `POST /session` capabilities for this dialect.
    pub fn create_session_body(self) -> Value {
        match self {
            Self::Uia2Server => json!({
                "capabilities": {
                    "alwaysMatch": { "platformName": "Android" },
                    "firstMatch": [{}]
                }
            }),
            Self::AppiumW3c => json!({
                "capabilities": {
                    "alwaysMatch": {
                        "platformName": "Android",
                        "appium:automationName": "UiAutomator2"
                    },
                    "firstMatch": [{}]
                }
            }),
        }
    }

    /// Build `POST …/element` locator body for this dialect.
    pub fn find_element_body(self, strategy: &str, selector: &str) -> Value {
        match self {
            Self::Uia2Server => json!({
                "strategy": strategy,
                "selector": selector,
                "context": "",
                "multiple": false
            }),
            Self::AppiumW3c => json!({
                "using": strategy,
                "value": selector
            }),
        }
    }

    /// Build `POST …/elements` locator body for this dialect.
    pub fn find_elements_body(self, strategy: &str, selector: &str) -> Value {
        match self {
            Self::Uia2Server => json!({
                "strategy": strategy,
                "selector": selector,
                "context": "",
                "multiple": true
            }),
            Self::AppiumW3c => json!({
                "using": strategy,
                "value": selector
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uia2_find_keeps_strategy_selector() {
        let b = SessionWireMode::Uia2Server.find_element_body("xpath", "//*[@text='Hi']");
        assert_eq!(b["strategy"], "xpath");
        assert_eq!(b["selector"], "//*[@text='Hi']");
        assert_eq!(b["multiple"], false);
        assert!(b.get("using").is_none());
    }

    #[test]
    fn appium_find_uses_using_value() {
        let b = SessionWireMode::AppiumW3c.find_element_body("id", "com.ex:id/btn");
        assert_eq!(b["using"], "id");
        assert_eq!(b["value"], "com.ex:id/btn");
        assert!(b.get("strategy").is_none());
        assert!(b.get("selector").is_none());
    }

    #[test]
    fn appium_elements_body_is_w3c() {
        let b = SessionWireMode::AppiumW3c.find_elements_body("xpath", "//Button");
        assert_eq!(b["using"], "xpath");
        assert_eq!(b["value"], "//Button");
        assert!(b.get("multiple").is_none());
    }

    #[test]
    fn appium_create_session_sets_automation_name() {
        let b = SessionWireMode::AppiumW3c.create_session_body();
        let always = &b["capabilities"]["alwaysMatch"];
        assert_eq!(always["platformName"], "Android");
        assert_eq!(always["appium:automationName"], "UiAutomator2");
    }

    #[test]
    fn uia2_create_session_stays_bare_android() {
        let b = SessionWireMode::Uia2Server.create_session_body();
        let always = &b["capabilities"]["alwaysMatch"];
        assert_eq!(always["platformName"], "Android");
        assert!(always.get("appium:automationName").is_none());
    }
}
