//! Router plane trait sketch — Wave H13/H10 spike.
//! HTTP `/v1/*` delegates to cliproxy++ (Go); combo logic stays here.

pub mod delegate;

/// Routing strategy for auto-combo variants (subset of OmniRoute spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboVariant {
    Auto,
    Coding,
    Fast,
    Cheap,
    Offline,
    Smart,
}

impl ComboVariant {
    pub fn parse(model_id: &str) -> Option<Self> {
        let suffix = model_id.strip_prefix("auto")?;
        match suffix {
            "" | "/" => Some(Self::Auto),
            "/coding" => Some(Self::Coding),
            "/fast" => Some(Self::Fast),
            "/cheap" => Some(Self::Cheap),
            "/offline" => Some(Self::Offline),
            "/smart" => Some(Self::Smart),
            _ => None,
        }
    }

    pub fn delegate_target(&self) -> &'static str {
        match self {
            Self::Auto | Self::Coding | Self::Smart => "cliproxy-delegate-quality",
            Self::Fast => "cliproxy-delegate-latency",
            Self::Cheap => "cliproxy-delegate-cost",
            Self::Offline => "cliproxy-delegate-quota",
        }
    }
}

pub trait RouterPlane {
    fn select_route(&self, model_id: &str) -> Option<String>;
}

pub struct ComboRouter;

impl RouterPlane for ComboRouter {
    fn select_route(&self, model_id: &str) -> Option<String> {
        ComboVariant::parse(model_id).map(|v| v.delegate_target().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_variants_delegate() {
        let r = ComboRouter;
        for id in [
            "auto",
            "auto/",
            "auto/coding",
            "auto/fast",
            "auto/cheap",
            "auto/offline",
            "auto/smart",
        ] {
            assert!(r.select_route(id).is_some(), "expected route for {id}");
        }
    }

    #[test]
    fn non_auto_returns_none() {
        let r = ComboRouter;
        assert!(r.select_route("gpt-4").is_none());
        assert!(r.select_route("auto/unknown").is_none());
    }

    #[test]
    fn variant_targets() {
        assert_eq!(
            ComboVariant::Coding.delegate_target(),
            "cliproxy-delegate-quality"
        );
        assert_eq!(
            ComboVariant::Fast.delegate_target(),
            "cliproxy-delegate-latency"
        );
    }
}
