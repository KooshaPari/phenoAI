//! L17 i18n runtime: `t(key)` returns the localized string from the
//! embedded locale table. Defaults to "en" if the key is not found.

use std::collections::HashMap;
use std::sync::OnceLock;

fn table() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        // Source of truth: locales/en.json
        m.insert("app.name".into(), "PhenoCompose".into());
        m.insert("app.tagline".into(), "Phenotype deployment composition.".into());
        m.insert("deploy.idle".into(), "Idle".into());
        m.insert("deploy.deploying".into(), "Deploying...".into());
        m.insert("deploy.running".into(), "Running".into());
        m.insert("deploy.failed".into(), "Failed".into());
        m.insert("menu.file".into(), "File".into());
        m.insert("menu.deploy".into(), "Deploy".into());
        m.insert("menu.help".into(), "Help".into());
        m.insert("error.network".into(), "Network unavailable.".into());
        m.insert("error.permission".into(), "Permission denied.".into());
        m.insert("error.not_found".into(), "Resource not found.".into());
        m.insert("ok.deployed".into(), "Deployed successfully.".into());
        m.insert("ok.connected".into(), "Connected.".into());
        m
    })
}

pub fn t(key: &str) -> &'static str {
    table().get(key).map(String::as_str).unwrap_or("?")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_key() {
        assert_eq!(t("app.name"), "PhenoCompose");
    }
    #[test]
    fn unknown_key() {
        assert_eq!(t("nope"), "?");
    }
    #[test]
    fn no_panic_on_empty() {
        assert_eq!(t(""), "?");
    }
}
